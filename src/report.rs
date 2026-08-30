//! Rendering a scan, for a person and for a machine.

use crate::lock::Tree;
use crate::rules::{self, Finding, Rule, Severity};
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
    // u64 because `24 * n` in u32 wraps at 178,956,971 findings and hands back
    // a *lower* number for a worse tree. Unreachable in practice; the fix is
    // one character and the reasoning about why it is unreachable is not.
    let n = findings.iter().filter(|f| f.severity == worst).count() as u64;
    floor + (24 * n / (n + 8)) as u32
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
    // Sanitized like everything else printed here: `scrub` covers what the
    // reader took out of the file, and this is the one display string that
    // comes from the filesystem instead. A directory named with an escape
    // sequence is a thing a hostile repo can contain.
    let file = tree.source.file_name().map_or_else(
        || tree.source.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let file = term::sanitize(&file);

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
    // One label per finding, kept alongside them — it used to be formatted
    // once to measure the column and a second time to print the row.
    let labels: Vec<String> = findings.iter().map(label).collect();
    let name_w = term::column(
        findings
            .iter()
            .zip(&labels)
            .filter(|(f, _)| shown(f))
            .map(|(_, l)| l.as_str()),
        NAME_MIN,
    );
    // The blocks are the rules that actually fired, sorted into report order —
    // taken from the findings rather than from a list of every rule, so there is
    // no list for a new rule to be left off of and no rule that can go missing
    // from the report without anyone noticing.
    let mut rules: Vec<Rule> = findings.iter().map(|f| f.rule).collect();
    rules.sort_unstable_by_key(|r| r.rank());
    rules.dedup();
    // The rules this file cannot answer are printed below the ones that fired,
    // and share their column — a heading in a different place would read as a
    // different kind of thing, and it is the same list of rules either way.
    let silent = rules::not_applicable(tree);
    let head_w = term::column(rules.iter().chain(&silent).map(|r| r.heading()), HEAD_MIN);

    for rule in rules {
        // Never empty: `rules` was built from the findings themselves, so
        // every rule in it has at least one. There used to be a guard here for
        // the case that cannot happen.
        let hits: Vec<(&Finding, &String)> = findings
            .iter()
            .zip(&labels)
            .filter(|(f, _)| f.rule == rule)
            .collect();
        let style = style_of(
            hits.iter()
                .fold(Severity::Low, |s, (f, _)| s.max(f.severity)),
        );
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
        if shown(hits[0].0) {
            for (f, label) in hits {
                writeln!(w, "     {} {}", term::pad(label, name_w), f.detail)?;
            }
        }
        writeln!(w)?;
    }

    // Below the findings, because it is not one. `stranger scan poetry.lock`
    // printed "no findings" over four rules, one of which had never been asked
    // — the file records no install-script flag, so the rule was reading a
    // column of `false` the reader invented. Silence about a question and
    // silence about an answer look identical in a terminal, and the exit code
    // is the same 0 either way; this line is the only thing that separates
    // them. It is not a finding: nothing here moves `risk` or `--fail-on`,
    // because an unasked question is not evidence.
    if !quiet && !silent.is_empty() {
        for rule in &silent {
            let line = format!(
                "  {}  {} {}",
                t.paint(Style::Dim, "·"),
                t.paint(Style::Dim, &term::pad(rule.heading(), head_w)),
                t.paint(Style::Dim, "— no signal in this format"),
            );
            writeln!(w, "{line}")?;
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
///
/// No elapsed time either, and that is the interesting omission. Two scans of
/// the same tree have to produce the same bytes or a diff between them is
/// noise, and `elapsed_ms` was the only field that changed run to run — which
/// made `diff <(stranger scan a --format json) <(stranger scan b --format
/// json)` — the recipe DECISIONS.md used to offer as the reason `stranger diff`
/// was cut — print a difference every single time. The human report still prints
/// the timing, because "41ms" is half the pitch and a person reading a
/// terminal is not diffing it.
pub fn json(w: &mut impl Write, tree: &Tree, findings: &[Finding]) -> io::Result<()> {
    write!(w, "{{\"source\":")?;
    string(w, &term::sanitize(&tree.source.display().to_string()))?;
    write!(w, ",\"ecosystem\":")?;
    string(w, tree.ecosystem.as_str())?;
    // `workspace` is here because it is the one header number a consumer could
    // not rebuild from the others: packages/direct/transitive are all
    // third-party counts, so nothing in them says how many first-party entries
    // the reader set aside. A monorepo and a flat project with the same
    // dependency count are indistinguishable in JSON without it.
    //
    // `integrity` is how many third-party entries recorded an integrity field
    // at all. Never whether one is *correct* — Rust std ships no crypto, so
    // there is no sha512 in this binary and there is not going to be. Every
    // reader computed this and nothing read it, which made the README's claim
    // that "stranger reports whether the field is present" false in the
    // honesty section of all places. Presence is half an answer; publishing
    // half an answer beats publishing none and calling it verified.
    write!(
        w,
        ",\"packages\":{},\"direct\":{},\"transitive\":{},\"workspace\":{},\"integrity\":{},\"risk\":{},\"findings\":[",
        tree.third_party(),
        tree.direct(),
        tree.transitive(),
        tree.workspace_members(),
        tree.with_integrity(),
        risk(findings),
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
    // The half of the answer an empty `findings` array cannot carry. A
    // consumer that treats `[]` as "clean" is right about the rules that ran
    // and wrong about the ones that could not, and on `poetry.lock` that is
    // two of the five. Always emitted, empty array included, so nobody has to
    // handle a missing key to find out whether the question was asked.
    write!(w, "],\"not_applicable\":[")?;
    for (i, rule) in rules::not_applicable(tree).into_iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        string(w, rule.id())?;
    }
    writeln!(w, "]}}")?;
    Ok(())
}

/// Writing JSON is not the same job as reading it, so this is not the parser
/// run backwards — it is eight lines that escape what RFC 8259 section 7
/// requires and nothing else.
///
/// `pub` rather than `pub(crate)` because `main` writes one JSON line of its
/// own — the blind-spot object, which is about the walk and so has no `Tree`
/// to hang off. Two escapers would be one escaper and one bug.
pub fn string(w: &mut impl Write, s: &str) -> io::Result<()> {
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

/// Rendering a diff.
///
/// Three blocks and then the findings, in that order, because a reviewer reads
/// "what moved" before "what that means". The findings block is the one the
/// exit code comes from, so it goes last and unabbreviated: a collapsed count
/// is fine for a scan of somebody else's 1,247 packages and wrong for the four
/// things this pull request did.
pub fn diff_human(
    w: &mut impl Write,
    t: Term,
    d: &crate::diff::Diff,
    quiet: bool,
) -> io::Result<()> {
    if !quiet {
        writeln!(w)?;
        writeln!(
            w,
            "  {} -> {}",
            term::sanitize(&d.old.source.display().to_string()),
            term::sanitize(&d.new.source.display().to_string()),
        )?;
        writeln!(w)?;
    }

    if d.is_empty() {
        if !quiet {
            writeln!(w, "  no change to the dependency tree")?;
        }
        return Ok(());
    }

    for (heading, items) in [("added", &d.added), ("removed", &d.removed)] {
        if items.is_empty() {
            continue;
        }
        writeln!(
            w,
            "  {}  {}",
            t.paint(Style::Dim, &term::pad(heading, 10)),
            thousands(items.len() as u64)
        )?;
        for item in items {
            writeln!(w, "     {}", term::sanitize(item))?;
        }
        writeln!(w)?;
    }

    if !d.changed.is_empty() {
        writeln!(
            w,
            "  {}  {}",
            t.paint(Style::Yellow, &term::pad("changed", 10)),
            thousands(d.changed.len() as u64)
        )?;
        for (name, from, to) in &d.changed {
            writeln!(
                w,
                "     {} {} -> {}",
                term::pad(&term::sanitize(name), NAME_MIN),
                term::sanitize(from),
                term::sanitize(to)
            )?;
        }
        writeln!(w)?;
    }

    // The half a reviewer is actually gating on. "resolved" is printed too,
    // because a change that fixes three things and introduces one is a
    // different conversation from one that only introduces one.
    for (heading, items) in [("introduced", &d.introduced), ("resolved", &d.resolved)] {
        if items.is_empty() {
            continue;
        }
        // Red for what this change added and dim for what it fixed. The scan
        // report colours by severity and nothing else, and a diff inventing a
        // green would be a second colour vocabulary for one command.
        let style = if heading == "introduced" {
            Style::Red
        } else {
            Style::Dim
        };
        writeln!(
            w,
            "  {}  {} finding{}",
            t.paint(style, &term::pad(heading, 10)),
            thousands(items.len() as u64),
            if items.len() == 1 { "" } else { "s" }
        )?;
        for f in items {
            writeln!(
                w,
                "     {} {}",
                term::pad(&label(f), NAME_MIN),
                t.paint(style_of(f.severity), &term::sanitize(&f.detail))
            )?;
        }
        writeln!(w)?;
    }

    if d.introduced.is_empty() && !quiet {
        writeln!(w, "  this change introduced no findings")?;
    }
    Ok(())
}

/// One object, not a stream: a diff is one comparison however many packages it
/// touched, so there is nothing to read a line at a time.
///
/// No timing field, for the reason `json` gives above — two runs over the same
/// pair have to produce the same bytes.
pub fn diff_json(w: &mut impl Write, d: &crate::diff::Diff) -> io::Result<()> {
    write!(w, "{{\"old\":")?;
    string(w, &d.old.source.display().to_string())?;
    write!(w, ",\"new\":")?;
    string(w, &d.new.source.display().to_string())?;

    for (key, items) in [("added", &d.added), ("removed", &d.removed)] {
        write!(w, ",\"{key}\":[")?;
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                write!(w, ",")?;
            }
            string(w, item)?;
        }
        write!(w, "]")?;
    }

    write!(w, ",\"changed\":[")?;
    for (i, (name, from, to)) in d.changed.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        write!(w, "{{\"name\":")?;
        string(w, name)?;
        write!(w, ",\"from\":")?;
        string(w, from)?;
        write!(w, ",\"to\":")?;
        string(w, to)?;
        write!(w, "}}")?;
    }
    write!(w, "]")?;

    for (key, items) in [("introduced", &d.introduced), ("resolved", &d.resolved)] {
        write!(w, ",\"{key}\":[")?;
        for (i, f) in items.iter().enumerate() {
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
        write!(w, "]")?;
    }

    writeln!(w, "}}")
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
