//! Rendering a scan, for a person and for a machine.

use crate::lock::Tree;
use crate::rules::{Finding, Rule, Severity};
use crate::term::{self, Style, Term};
use std::io::{self, Write};
use std::time::Duration;

/// Column floors, not fixed widths — `term::column` grows past them when a
/// name needs the room. They exist because a scan with three short names
/// should not collapse into a cramped pair of columns, and because holding
/// them steady keeps two scans of the same project diffable.
const NAME_MIN: usize = 24;
const HEAD_MIN: usize = 22;

/// A band for the worst severity, and position inside it for volume.
///
/// This was a sum of severity weights capped at 100, and the cap did all the
/// work: nine of the sixteen fixtures scored exactly 100, including both
/// `poisoned.package-lock.json` and the clean `npm-l` it was built from. A
/// number that cannot separate three planted hallucinations from its own
/// control is not telling anyone anything.
///
/// So the band is the worst severity present — the same question `--fail-on`
/// asks, which is the point: the headline number and the gate should not
/// disagree about what is serious. Position inside the band is how many
/// findings share that severity.
///
/// It is still not calibrated against anything, because there is nothing
/// honest to calibrate it against. The findings are the output. This is a
/// handle for `--fail-on` to sit beside and for a repeated scan to show
/// movement against, and comparing two projects is only meaningful at the
/// band.
pub fn risk(findings: &[Finding]) -> u32 {
    let Some(worst) = findings.iter().map(|f| f.severity).max() else {
        return 0;
    };
    let floor = match worst {
        Severity::Critical => 75,
        Severity::High => 50,
        Severity::Medium => 25,
        Severity::Low => 1,
    };
    // Saturating, so the band cannot be filled: one finding sits near the
    // floor, a dozen most of the way up, and a thousand still leaves room,
    // because there is always a worse tree than the one in front of you.
    // Integer arithmetic throughout — this number is displayed, not computed
    // with, and a float here would only invite rounding questions.
    let n = findings.iter().filter(|f| f.severity == worst).count() as u32;
    floor + 24 * n / (n + 8)
}

/// Digit grouping. `NumBuffer` + `format_into` (1.98) writes the digits into a
/// stack buffer with no allocation and no `Display` machinery; the separators
/// go in on the way out.
pub fn thousands(n: u64) -> String {
    let mut buf = core::fmt::NumBuffer::<u64>::new();
    let digits = n.format_into(&mut buf);
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let first = digits.len() % 3;
    for (i, c) in digits.char_indices() {
        if i > 0 && i % 3 == first {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The tail of a rule's summary line, for rules that do not list every hit.
///
/// A 1,390-package tree produces 76 drift findings and 29 trivial ones. Printing
/// all of them buries the three that matter under a hundred lines nobody scrolls
/// back through, so only critical findings are listed by default and everything
/// else reports a count and what the count means. `--verbose` prints the lot.
fn note(rule: Rule, hits: usize, tree: &Tree) -> String {
    match rule {
        Rule::Slopsquat => String::new(),
        Rule::Trivial => {
            // Against third_party(), not packages.len(), because the rule skips
            // first-party packages outright — a workspace member can never be a
            // hit, so counting it in the denominator prints a percentage of a
            // population the numerator was never drawn from. On npm-m that is
            // 2.9% against 3.0%, which is small enough to have survived a while
            // and wrong in the direction that flatters the number.
            let pct = 100.0 * hits as f64 / tree.third_party().max(1) as f64;
            format!("({pct:.1}% of third-party)")
        }
        Rule::InstallScript => "arbitrary code at install time".into(),
        Rule::Drift => "same package at 2+ versions in one tree".into(),
        Rule::Pinning => "no exact version recorded".into(),
    }
}

pub fn human(
    w: &mut impl Write,
    t: Term,
    tree: &Tree,
    findings: &[Finding],
    elapsed: Duration,
    verbose: bool,
    quiet: bool,
) -> io::Result<()> {
    let file = tree.source.file_name().map_or_else(
        || tree.source.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );

    if !quiet {
        let workspace = match tree.workspace_members() {
            0 => String::new(),
            n => format!(" · {} workspace", thousands(n as u64)),
        };
        writeln!(w)?;
        writeln!(
            w,
            "  {} {} packages   ({} direct · {} transitive{workspace})",
            term::pad(&file, NAME_MIN),
            thousands(tree.third_party() as u64),
            thousands(tree.direct() as u64),
            thousands(tree.transitive() as u64),
        )?;
        writeln!(w)?;
    }

    if findings.is_empty() && !quiet {
        writeln!(w, "  no findings")?;
    }

    // Measured across every finding that will actually be printed, rather than
    // per rule block, so the detail column is one column down the whole report
    // and not a staircase. Findings that stay collapsed do not get a vote —
    // otherwise one long trivial-package name widens a column nobody sees.
    let shown = |f: &Finding| verbose || f.rule == Rule::Slopsquat;
    let labels: Vec<String> = findings.iter().filter(|f| shown(f)).map(label).collect();
    let name_w = term::column(labels.iter().map(String::as_str), NAME_MIN);
    // The blocks are the rules that actually fired, sorted into report order —
    // taken from the findings rather than from a list of every rule, so there is
    // no list for a new rule to be left off of and no rule that can go missing
    // from the report without anyone noticing.
    let mut rules: Vec<Rule> = findings.iter().map(|f| f.rule).collect();
    rules.sort_unstable_by_key(|r| r.rank());
    rules.dedup();
    let head_w = term::column(rules.iter().map(|r| r.heading()), HEAD_MIN);

    for rule in rules {
        let hits: Vec<&Finding> = findings.iter().filter(|f| f.rule == rule).collect();
        if hits.is_empty() {
            continue;
        }
        let style = style_of(hits.iter().fold(Severity::Low, |s, f| s.max(f.severity)));
        // Pad first, paint second. The escape codes are bytes in the string,
        // and a column measured over a painted cell is a column measured over
        // eight characters nobody can see.
        let count = hits.len();
        let tail = note(rule, count, tree);
        let line = format!(
            "  {}  {} {:<5} {}",
            t.paint(style, "⚠"),
            t.paint(style, &term::pad(rule.heading(), head_w)),
            count,
            tail,
        );
        writeln!(w, "{}", line.trim_end())?;
        // Critical findings always get their lines — they are the answer, and
        // there are never many. The rest are a count until asked.
        if shown(hits[0]) {
            for f in hits {
                writeln!(w, "     {} {}", term::pad(&label(f), name_w), f.detail)?;
            }
        }
        writeln!(w)?;
    }

    if !quiet {
        writeln!(
            w,
            "  risk {}/100    {}ms    third-party deps used to compute this: 0",
            risk(findings),
            elapsed.as_millis(),
        )?;
        writeln!(w)?;
    }
    Ok(())
}

fn label(f: &Finding) -> String {
    if f.version.is_empty() {
        f.package.clone()
    } else {
        format!("{}@{}", f.package, f.version)
    }
}

fn style_of(sev: Severity) -> Style {
    match sev {
        Severity::Critical => Style::Red,
        Severity::High => Style::Orange,
        Severity::Medium => Style::Yellow,
        Severity::Low => Style::Dim,
    }
}

/// No `Term` here, deliberately. JSON goes to a program, and a program that
/// has to strip SGR codes out of a string field will not.
pub fn json(
    w: &mut impl Write,
    tree: &Tree,
    findings: &[Finding],
    elapsed: Duration,
) -> io::Result<()> {
    write!(w, "{{\"source\":")?;
    string(w, &tree.source.display().to_string())?;
    write!(w, ",\"ecosystem\":")?;
    string(w, tree.ecosystem.as_str())?;
    // `workspace` is here because it is the one header number a consumer could
    // not rebuild from the others: packages/direct/transitive are all
    // third-party counts, so nothing in them says how many first-party entries
    // the reader set aside. A monorepo and a flat project with the same
    // dependency count are indistinguishable in JSON without it.
    write!(
        w,
        ",\"packages\":{},\"direct\":{},\"transitive\":{},\"workspace\":{},\"risk\":{},\"elapsed_ms\":{},\"findings\":[",
        tree.third_party(),
        tree.direct(),
        tree.transitive(),
        tree.workspace_members(),
        risk(findings),
        elapsed.as_millis(),
    )?;
    for (i, f) in findings.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        write!(w, "{{\"rule\":")?;
        string(w, f.rule.id())?;
        write!(w, ",\"severity\":")?;
        string(w, &f.severity.to_string())?;
        write!(w, ",\"package\":")?;
        string(w, &f.package)?;
        write!(w, ",\"version\":")?;
        string(w, &f.version)?;
        write!(w, ",\"detail\":")?;
        string(w, &f.detail)?;
        write!(w, "}}")?;
    }
    writeln!(w, "]}}")?;
    Ok(())
}

/// Writing JSON is not the same job as reading it, so this is not the parser
/// run backwards — it is eight lines that escape what RFC 8259 section 7
/// requires and nothing else.
pub(crate) fn string(w: &mut impl Write, s: &str) -> io::Result<()> {
    write!(w, "\"")?;
    for c in s.chars() {
        match c {
            '"' => write!(w, "\\\"")?,
            '\\' => write!(w, "\\\\")?,
            '\n' => write!(w, "\\n")?,
            '\r' => write!(w, "\\r")?,
            '\t' => write!(w, "\\t")?,
            c if (c as u32) < 0x20 => write!(w, "\\u{:04x}", c as u32)?,
            c => write!(w, "{c}")?,
        }
    }
    write!(w, "\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(sev: Severity, n: usize) -> Vec<Finding> {
        (0..n)
            .map(|i| Finding {
                rule: Rule::Slopsquat,
                severity: sev,
                package: format!("p{i}"),
                version: String::new(),
                detail: String::new(),
            })
            .collect()
    }

    #[test]
    fn nothing_scores_zero() {
        assert_eq!(risk(&[]), 0);
    }

    /// The bands must not overlap, or the number disagrees with `--fail-on`
    /// about what is serious — which is the whole reason they are bands.
    #[test]
    fn a_worse_severity_always_outranks_more_of_a_lesser_one() {
        assert!(risk(&at(Severity::Critical, 1)) > risk(&at(Severity::High, 10_000)));
        assert!(risk(&at(Severity::High, 1)) > risk(&at(Severity::Medium, 10_000)));
        assert!(risk(&at(Severity::Medium, 1)) > risk(&at(Severity::Low, 10_000)));
        assert!(risk(&at(Severity::Low, 1)) > risk(&[]));
    }

    /// The old score summed weights and capped at 100, so anything with two
    /// rules firing hit the ceiling and stayed there. Nine of sixteen fixtures
    /// scored exactly 100.
    #[test]
    fn volume_moves_the_number_without_ever_filling_the_band() {
        let (one, ten, many) = (
            risk(&at(Severity::High, 1)),
            risk(&at(Severity::High, 10)),
            risk(&at(Severity::High, 100_000)),
        );
        assert!(one < ten && ten < many, "{one} {ten} {many}");
        assert!(many < 75, "a High tree must never reach the Critical band");
    }

    /// A findings list is not sorted by severity on the way in here, so the
    /// band has to come from a max rather than from the first element.
    #[test]
    fn the_band_comes_from_the_worst_finding_not_the_first() {
        let mut mixed = at(Severity::Low, 3);
        mixed.extend(at(Severity::Critical, 1));
        assert_eq!(risk(&mixed), risk(&at(Severity::Critical, 1)));
    }
}
