//! Rendering a scan, for a person and for a machine.

use crate::lock::Tree;
use crate::rules::{Finding, Rule, Severity};
use std::io::{self, Write};
use std::time::Duration;

/// Severity weights, capped at 100.
///
/// This is a crude number and saying so is better than dressing it up. It
/// exists so `--fail-on` has something to compare and so a repeated scan shows
/// movement; it is not calibrated against anything, because there is nothing
/// honest to calibrate it against. The findings are the output. The score is a
/// handle.
pub fn risk(findings: &[Finding]) -> u32 {
    let total: u32 = findings
        .iter()
        .map(|f| match f.severity {
            Severity::Critical => 25,
            Severity::High => 10,
            Severity::Medium => 3,
            Severity::Low => 1,
        })
        .sum();
    total.min(100)
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

pub fn human(
    w: &mut impl Write,
    tree: &Tree,
    findings: &[Finding],
    elapsed: Duration,
) -> io::Result<()> {
    let file = tree.source.file_name().map_or_else(
        || tree.source.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );

    writeln!(w)?;
    writeln!(
        w,
        "  {:<24} {} packages   ({} direct · {} transitive)",
        file,
        thousands(tree.packages.len() as u64),
        thousands(tree.direct() as u64),
        thousands(tree.transitive() as u64),
    )?;
    writeln!(w)?;

    if findings.is_empty() {
        writeln!(w, "  no findings")?;
    }

    for rule in [Rule::Slopsquat] {
        let hits: Vec<&Finding> = findings.iter().filter(|f| f.rule == rule).collect();
        if hits.is_empty() {
            continue;
        }
        writeln!(w, "  ⚠  {:<22} {}", rule.heading(), hits.len())?;
        for f in hits {
            let name = if f.version.is_empty() {
                f.package.clone()
            } else {
                format!("{}@{}", f.package, f.version)
            };
            writeln!(w, "     {:<24} {}", name, f.detail)?;
        }
        writeln!(w)?;
    }

    writeln!(
        w,
        "  risk {}/100    {}ms    third-party deps used to compute this: 0",
        risk(findings),
        elapsed.as_millis(),
    )?;
    writeln!(w)?;
    Ok(())
}

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
    write!(
        w,
        ",\"packages\":{},\"direct\":{},\"transitive\":{},\"risk\":{},\"elapsed_ms\":{},\"findings\":[",
        tree.packages.len(),
        tree.direct(),
        tree.transitive(),
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
fn string(w: &mut impl Write, s: &str) -> io::Result<()> {
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
