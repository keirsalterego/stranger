//! `stranger tree <pkg>` — clause 3, as something you can look at.
//!
//! The first two clauses of the co-occurrence rule are checkable by hand: a
//! name is in the corpus or it is not, and an edit distance is arithmetic. The
//! third one — *nothing in the lockfile depends on it* — is a claim about a
//! graph nobody can see, so until now the only thing to do with it was believe
//! the report. This prints the graph around one name: every edge into it, the
//! count of them, and every edge out.

use crate::corpus;
use crate::distance::{self, MAX_EDIT_DISTANCE};
use crate::lock::{Package, Tree};
use crate::report;
use crate::semver::Version;
use crate::term::{self, Style, Term};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};
use std::io::{self, Write};
use std::path::Path;

/// How deep the out-edge tree prints before it stops and says so.
///
/// Not a safety limit. The repeat marking in `expand` is what bounds the walk,
/// and `--depth 0` is a supported thing to type. This is a readability one:
/// `express` in `npm-l` is 28 direct dependencies, 86 lines at three levels and
/// 123 with no limit, and the extra 37 are the fifth and sixth levels of a
/// transitive closure nobody reads. Three is enough to see what a package
/// actually pulls in and short enough to stay on a screen while somebody says a
/// sentence about it.
pub const DEFAULT_DEPTH: usize = 3;

/// How many near names to list when the package is not there. Enough to spot
/// the one you meant, few enough that the answer is still "it is not here".
const MAX_NEAR: usize = 8;

/// Left column for the section labels. `depended on by` is the widest.
const LABEL: usize = 17;

/// Name column floor in the near-miss list, the same one `report` uses for a
/// finding, so the two lists line up in a terminal.
const NEAR_MIN: usize = 24;

/// One place the name turned up: a lockfile, and one entry in it.
pub struct Hit<'a> {
    pub tree: &'a Tree,
    pub index: usize,
}

impl Hit<'_> {
    fn pkg(&self) -> &Package {
        &self.tree.packages[self.index]
    }

    /// Every package with an edge into this one, deduplicated and ordered.
    /// Its length is the in-degree the slopsquat rule reads.
    fn parents(&self) -> Vec<usize> {
        let mut from: Vec<usize> = self
            .tree
            .edges
            .iter()
            .filter(|&&(_, to)| to == self.index)
            .map(|&(from, _)| from)
            .collect();
        from.sort_unstable_by(|&a, &b| order(&self.tree.packages, a, b));
        from.dedup();
        from
    }
}

/// A name that is in the trees when the one you asked for is not.
pub struct Near {
    pub name: String,
    pub source: String,
    pub distance: usize,
}

/// One answer to one question about one name, ready to render.
pub struct Report<'a> {
    pub query: &'a str,
    /// What the user pointed at, for the sentence when nothing matches.
    pub root: &'a Path,
    pub scanned: usize,
    pub depth: usize,
    pub hits: Vec<Hit<'a>>,
    pub near: Vec<Near>,
}

impl<'a> Report<'a> {
    pub fn build(trees: &'a [Tree], query: &'a str, root: &'a Path, depth: usize) -> Self {
        let hits = resolve(trees, query);
        // Only worth the corpus-free distance sweep when there is nothing to
        // show. A found package needs no suggestions.
        let near = if hits.is_empty() {
            near(trees, query)
        } else {
            Vec::new()
        };
        Report {
            query,
            root,
            scanned: trees.len(),
            depth,
            hits,
            near,
        }
    }
}

/// Every entry in every tree whose name is `query`.
///
/// Not the first match and not the newest. Version drift is one of the four
/// things this tool reports, and npm spells a duplicated package as a second
/// entry under a nested key, so one name at three versions is three hits and
/// all three get printed. Silently picking one would hide the finding a scan
/// of the same file would have raised.
///
/// Matching is on the ecosystem's normalised form, so `Flask` and `flask` and
/// `flask` spelled with an underscore are one package on PyPI — the same
/// normalisation the corpus lookup uses, rather than a second opinion about
/// what two names being equal means.
pub fn resolve<'a>(trees: &'a [Tree], query: &str) -> Vec<Hit<'a>> {
    let mut hits = Vec::new();
    for tree in trees {
        let want = corpus::normalize(tree.ecosystem, query);
        let mut found: Vec<usize> = tree
            .packages
            .iter()
            .enumerate()
            .filter(|(_, p)| corpus::normalize(tree.ecosystem, &p.name) == want)
            .map(|(i, _)| i)
            .collect();
        found.sort_by(|&a, &b| order(&tree.packages, a, b));
        hits.extend(found.into_iter().map(|index| Hit { tree, index }));
    }
    hits
}

/// Names in the scanned trees within `MAX_EDIT_DISTANCE` of `query`.
///
/// The same distance function and the same threshold the slopsquat rule uses,
/// so "close" here means what it means in a finding. The set collapses the
/// dozens of duplicate entries one name has in a big lockfile and sorts the
/// answer on the way out.
pub fn near(trees: &[Tree], query: &str) -> Vec<Near> {
    let mut found = BTreeSet::new();
    for tree in trees {
        let want = corpus::normalize(tree.ecosystem, query);
        let source = tree.source.display().to_string();
        for pkg in &tree.packages {
            let name = corpus::normalize(tree.ecosystem, &pkg.name);
            if let Some(d) = distance::within(&want, &name, MAX_EDIT_DISTANCE) {
                found.insert((d, pkg.name.clone(), source.clone()));
            }
        }
    }
    found
        .into_iter()
        .take(MAX_NEAR)
        .map(|(distance, name, source)| Near {
            name,
            source,
            distance,
        })
        .collect()
}

/// Why the walk stopped at a node instead of expanding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// This name is already open on the path above it.
    Cycle,
    /// Its dependencies were printed in full somewhere earlier.
    Repeat,
    /// `--depth`, carrying the number of direct dependencies not shown.
    Depth(usize),
}

impl Stop {
    fn id(self) -> &'static str {
        match self {
            Stop::Cycle => "cycle",
            Stop::Repeat => "repeat",
            Stop::Depth(_) => "depth",
        }
    }
}

/// One node of the out-edge walk. Built once, rendered twice.
pub struct Node {
    pub index: usize,
    pub stop: Option<Stop>,
    pub deps: Vec<Node>,
}

struct Graph {
    kids: Vec<Vec<usize>>,
    limit: usize,
}

impl Graph {
    /// ponytail: rebuilt per hit, so a name at four versions in one lockfile
    /// walks the edge list four times. That is O(V+E) on a list of a few
    /// thousand and it is not the slow part of anything; hoist it into the
    /// caller if a lockfile ever turns up where it is.
    fn new(tree: &Tree, limit: usize) -> Self {
        let mut kids = vec![Vec::new(); tree.packages.len()];
        for &(from, to) in &tree.edges {
            kids[from].push(to);
        }
        // Sorted, so two runs over one lockfile emit the same bytes. The
        // reader's edge order is the order the file happened to list things
        // in, which is not a promise anybody made.
        for row in &mut kids {
            row.sort_by(|&a, &b| order(&tree.packages, a, b));
        }
        Graph { kids, limit }
    }
}

/// The out-edge walk, and the two things that stop it.
///
/// A lockfile graph is a DAG, and a real one has cycles in it as well — npm
/// records peer dependencies in both directions often enough that `a → b → a`
/// is ordinary. Printing every path through a DAG is exponential in the depth
/// and printing a cycle does not terminate at all, so a node's dependencies
/// are expanded exactly once: the second time it is reached it prints as a
/// leaf, marked `cycle` when it is still open on the path above and `(*)` when
/// it was finished somewhere earlier. That distinction is the whole reason
/// `seen` and `path` are two collections and not one — `seen` bounds the work,
/// `path` is what makes the label honest.
///
/// Both markers reach the output. Truncating a dependency tree without saying
/// where is how a reader ends up believing a package depends on less than it
/// does, which in this tool is the same failure mode as the rule it exists to
/// demonstrate.
fn expand(g: &Graph, seen: &mut HashSet<usize>, path: &mut Vec<usize>, node: usize) -> Vec<Node> {
    let mut out = Vec::with_capacity(g.kids[node].len());
    for &kid in &g.kids[node] {
        let below = g.kids[kid].len();
        // Order matters here. The depth cut is checked before `seen`, and
        // `seen` is only written when the node is about to be expanded — a
        // node first met at the depth limit must not be recorded as printed,
        // or every later occurrence of it, however shallow, comes out as a
        // bare `(*)` pointing at a subtree that was never printed anywhere.
        let stop = if path.contains(&kid) {
            Some(Stop::Cycle)
        } else if g.limit != 0 && path.len() >= g.limit {
            (below > 0).then_some(Stop::Depth(below))
        } else if !seen.insert(kid) {
            Some(Stop::Repeat)
        } else {
            None
        };
        let deps = if stop.is_some() {
            Vec::new()
        } else {
            path.push(kid);
            let deps = expand(g, seen, path, kid);
            path.pop();
            deps
        };
        out.push(Node {
            index: kid,
            stop,
            deps,
        });
    }
    out
}

fn walk(tree: &Tree, root: usize, limit: usize) -> (Vec<Node>, usize) {
    let g = Graph::new(tree, limit);
    let direct = g.kids[root].len();
    let mut seen = HashSet::from([root]);
    let mut path = vec![root];
    (expand(&g, &mut seen, &mut path, root), direct)
}

/// Name, version, key, index — a total order over entries that does not depend
/// on which order the reader happened to produce them in.
fn order(pkgs: &[Package], a: usize, b: usize) -> Ordering {
    let (x, y) = (&pkgs[a], &pkgs[b]);
    x.name
        .cmp(&y.name)
        // Semver where both sides parse, so 10.1.0 sorts after 7.0.1 rather
        // than one character at a time. `rules::drift` sorts the same way and
        // for the same reason.
        .then_with(
            || match (Version::parse(&x.version), Version::parse(&y.version)) {
                (Some(p), Some(q)) => p.cmp(&q),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
        )
        .then_with(|| x.version.cmp(&y.version))
        .then_with(|| x.key.cmp(&y.key))
        .then(a.cmp(&b))
}

fn label(pkg: &Package) -> String {
    if pkg.version.is_empty() {
        pkg.name.clone()
    } else {
        format!("{}@{}", pkg.name, pkg.version)
    }
}

pub fn human(w: &mut impl Write, t: Term, r: &Report<'_>, quiet: bool) -> io::Result<()> {
    if r.hits.is_empty() {
        return missing(w, t, r);
    }

    let mut last: Option<&Path> = None;
    for hit in &r.hits {
        let source = hit.tree.source.as_path();
        if last != Some(source) && !quiet {
            writeln!(w)?;
            writeln!(
                w,
                "  {}   {} · {} packages",
                source.display(),
                hit.tree.ecosystem.as_str(),
                report::thousands(hit.tree.third_party() as u64),
            )?;
        }
        last = Some(source);
        block(w, t, hit, r.depth, quiet)?;
    }
    writeln!(w)
}

fn block(w: &mut impl Write, t: Term, hit: &Hit<'_>, limit: usize, quiet: bool) -> io::Result<()> {
    let pkg = hit.pkg();
    writeln!(w)?;
    // The lockfile's own key, but only when it says something the name does
    // not. On npm it is the install path and carries the nesting that made
    // this a second copy. pnpm writes `name@version`, Cargo writes
    // `name version`, and the two flat readers write the bare name — all three
    // are already on this line, so only npm's key earns its place.
    let shown = label(pkg);
    let redundant = pkg.key == pkg.name
        || pkg.key == shown
        || pkg.key == format!("{} {}", pkg.name, pkg.version);
    let key = if redundant {
        String::new()
    } else {
        format!("   {}", t.paint(Style::Dim, &pkg.key))
    };
    writeln!(w, "  {shown}{key}")?;
    if pkg.first_party {
        writeln!(
            w,
            "     {}",
            t.paint(
                Style::Dim,
                "workspace member — your own code, not a stranger"
            ),
        )?;
    }
    writeln!(w)?;

    if !hit.tree.records_edges {
        return no_graph(w, quiet);
    }

    let parents = hit.parents();
    let degree = parents.len();
    if degree == 0 {
        // `root-only, no parent` is the phrase the finding prints, so the
        // report, the README and the rule say one thing rather than three. It
        // belongs only on an entry the rule would look at: first-party code is
        // skipped outright by `slopsquat::scan`, and reciting a clause at a
        // crate you wrote yourself is explaining a rule that never ran.
        let headline = if pkg.first_party {
            t.paint(Style::Dim, "in-degree 0")
        } else {
            t.paint(Style::Red, "in-degree 0 · root-only, no parent")
        };
        writeln!(w, "     {}{headline}", term::pad("depended on by", LABEL))?;
        if !quiet {
            // Hard-wrapped at something that fits an 80-column terminal once
            // the label column is in front of it. Reflowing to the real width
            // would mean asking the terminal how wide it is, which is a
            // `TIOCGWINSZ` ioctl and therefore an `unsafe` block.
            let prose: &[&str] = if pkg.first_party {
                &[
                    "nothing in this lockfile depends on it, which for your",
                    "own code is the ordinary shape. The name rules skip",
                    "first-party entries, so clause 3 of the co-occurrence",
                    "rule is never asked about this one.",
                ]
            } else if hit.tree.roots.contains(&hit.index) {
                &[
                    "nothing in this lockfile depends on it. The only",
                    "reference to the name in the file is the manifest under",
                    "audit. That is clause 3 of the co-occurrence rule: a",
                    "hallucinated package is a root dependency, because",
                    "nothing real has ever heard of it.",
                ]
            } else {
                &[
                    "nothing in this lockfile depends on it, and no manifest",
                    "here names it either. That is clause 3 of the",
                    "co-occurrence rule: a hallucinated package is a root",
                    "dependency, because nothing real has ever heard of it.",
                ]
            };
            for line in prose {
                writeln!(
                    w,
                    "     {}{}",
                    term::pad("", LABEL),
                    t.paint(Style::Dim, line)
                )?;
            }
        }
    } else {
        writeln!(
            w,
            "     {}in-degree {degree}",
            term::pad("depended on by", LABEL),
        )?;
        // Five parents can be three distinct entries called `debug@3.2.7`,
        // because npm nests a copy per install path and each copy is its own
        // node with its own edges. The in-degree is right and the list looks
        // like a rendering fault, so the install path goes on the end of the
        // lines that would otherwise be indistinguishable — and only those.
        let labels: Vec<String> = parents
            .iter()
            .map(|&i| label(&hit.tree.packages[i]))
            .collect();
        for (&i, name) in parents.iter().zip(&labels) {
            let ambiguous = labels.iter().filter(|l| *l == name).count() > 1;
            let key = if ambiguous {
                format!("   {}", t.paint(Style::Dim, &hit.tree.packages[i].key))
            } else {
                String::new()
            };
            writeln!(w, "     {}{name}{key}", term::pad("", LABEL))?;
        }
    }
    writeln!(w)?;

    let (nodes, direct) = walk(hit.tree, hit.index, limit);
    let count = match (direct, limit) {
        (0, _) => "nothing".to_string(),
        (n, 0) => format!("{n} direct, all the way down"),
        (n, d) => format!("{n} direct, to depth {d}"),
    };
    writeln!(w, "     {}{count}", term::pad("depends on", LABEL))?;
    let mut pen = Pen {
        t,
        tree: hit.tree,
        limit,
        marked: false,
    };
    for (i, node) in nodes.iter().enumerate() {
        pen.branch(w, node, "     ", i + 1 == nodes.len())?;
    }
    if pen.marked && !quiet {
        writeln!(
            w,
            "     {}",
            t.paint(
                Style::Dim,
                "(*) its dependencies are printed above — a lockfile is a graph, not a tree",
            ),
        )?;
    }
    Ok(())
}

/// The out-edge tree, drawn. State rather than six arguments threaded through
/// a recursion: `marked` is an answer the whole walk contributes to, and it
/// only matters once, at the bottom, as a legend line.
struct Pen<'a> {
    t: Term,
    tree: &'a Tree,
    limit: usize,
    marked: bool,
}

impl Pen<'_> {
    fn branch(
        &mut self,
        w: &mut impl Write,
        node: &Node,
        prefix: &str,
        last: bool,
    ) -> io::Result<()> {
        let stem = if last { "└─ " } else { "├─ " };
        let note = match node.stop {
            None => String::new(),
            Some(Stop::Cycle) => " · cycle, back to a name already above it".to_string(),
            Some(Stop::Repeat) => {
                self.marked = true;
                " (*)".to_string()
            }
            Some(Stop::Depth(n)) => format!(" · {n} more below, past --depth {}", self.limit),
        };
        writeln!(
            w,
            "{prefix}{}{}{}",
            self.t.paint(Style::Dim, stem),
            label(&self.tree.packages[node.index]),
            self.t.paint(Style::Dim, &note),
        )?;
        let below = format!("{prefix}{}", if last { "   " } else { "│  " });
        for (i, kid) in node.deps.iter().enumerate() {
            self.branch(w, kid, &below, i + 1 == node.deps.len())?;
        }
        Ok(())
    }
}

/// The flat-format answer. README LIMITS calls this one out by name and this
/// says the same thing: the absence of edges here is the file declining to
/// record any, not a measurement of zero.
fn no_graph(w: &mut impl Write, quiet: bool) -> io::Result<()> {
    writeln!(
        w,
        "     {}no graph in this file",
        term::pad("flat format", LABEL)
    )?;
    if !quiet {
        for line in [
            "requirements.txt records no dependency edges at all, so",
            "there is no in-degree here to read and no out-edges to",
            "walk. Every package in it trivially has in-degree 0,",
            "which is why clause 3 is vacuous on a flat file and the",
            "rule falls back to two clauses. Point this at a",
            "poetry.lock or a uv.lock and there is a graph to look at.",
        ] {
            writeln!(w, "     {}{line}", term::pad("", LABEL))?;
        }
    }
    Ok(())
}

fn missing(w: &mut impl Write, t: Term, r: &Report<'_>) -> io::Result<()> {
    writeln!(w)?;
    writeln!(
        w,
        "  no package named `{}` in the {} lockfile{} under {}",
        r.query,
        r.scanned,
        if r.scanned == 1 { "" } else { "s" },
        r.root.display(),
    )?;
    if r.near.is_empty() {
        writeln!(
            w,
            "  {}",
            t.paint(
                Style::Dim,
                &format!("nothing within {MAX_EDIT_DISTANCE} edits of it either"),
            ),
        )?;
        writeln!(w)?;
        return Ok(());
    }
    writeln!(w)?;
    writeln!(w, "  close names that are there:")?;
    let width = term::column(r.near.iter().map(|n| n.name.as_str()), NEAR_MIN);
    for n in &r.near {
        writeln!(
            w,
            "     {} {}",
            term::pad(&n.name, width),
            t.paint(Style::Dim, &format!("d={} · {}", n.distance, n.source)),
        )?;
    }
    writeln!(w)?;
    Ok(())
}

/// One object, not one per lockfile.
///
/// `scan` answers a question per file, so it streams a line per file. `tree`
/// answers one question about one name and the answer includes "it is in these
/// three files" and "it is in none of them" — which is not a thing a stream of
/// per-file objects can say. So it is a single object, on a single line, and a
/// consumer reads it whole.
pub fn json(w: &mut impl Write, r: &Report<'_>) -> io::Result<()> {
    write!(w, "{{\"query\":")?;
    report::string(w, r.query)?;
    write!(
        w,
        ",\"found\":{},\"lockfiles\":{},\"depth\":{},\"occurrences\":[",
        !r.hits.is_empty(),
        r.scanned,
        r.depth,
    )?;
    for (i, hit) in r.hits.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        occurrence(w, hit, r.depth)?;
    }
    write!(w, "],\"near\":[")?;
    for (i, n) in r.near.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        write!(w, "{{\"name\":")?;
        report::string(w, &n.name)?;
        write!(w, ",\"distance\":{},\"source\":", n.distance)?;
        report::string(w, &n.source)?;
        write!(w, "}}")?;
    }
    writeln!(w, "]}}")
}

fn occurrence(w: &mut impl Write, hit: &Hit<'_>, limit: usize) -> io::Result<()> {
    let pkg = hit.pkg();
    write!(w, "{{\"source\":")?;
    report::string(w, &hit.tree.source.display().to_string())?;
    write!(w, ",\"ecosystem\":")?;
    report::string(w, hit.tree.ecosystem.as_str())?;
    write!(w, ",\"name\":")?;
    report::string(w, &pkg.name)?;
    write!(w, ",\"version\":")?;
    report::string(w, &pkg.version)?;
    write!(w, ",\"key\":")?;
    report::string(w, &pkg.key)?;
    write!(
        w,
        ",\"first_party\":{},\"direct\":{},\"records_edges\":{}",
        pkg.first_party,
        hit.tree.roots.contains(&hit.index),
        hit.tree.records_edges,
    )?;

    // `in_degree` is null and not 0 on a flat format, because the file records
    // no edges and 0 would be a measurement nobody took.
    if !hit.tree.records_edges {
        return write!(
            w,
            ",\"in_degree\":null,\"parents\":[],\"dependencies\":[]}}"
        );
    }

    let parents = hit.parents();
    write!(w, ",\"in_degree\":{},\"parents\":[", parents.len())?;
    for (i, &p) in parents.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        entry(w, &hit.tree.packages[p])?;
        write!(w, "}}")?;
    }
    write!(w, "],\"dependencies\":[")?;
    let (nodes, _) = walk(hit.tree, hit.index, limit);
    nodes_json(w, hit.tree, &nodes)?;
    write!(w, "]}}")
}

/// Opens an object with the two fields every node carries and leaves it open,
/// so the caller can add its own before closing. Saves a third spelling of
/// name-and-version.
fn entry(w: &mut impl Write, pkg: &Package) -> io::Result<()> {
    write!(w, "{{\"name\":")?;
    report::string(w, &pkg.name)?;
    write!(w, ",\"version\":")?;
    report::string(w, &pkg.version)
}

fn nodes_json(w: &mut impl Write, tree: &Tree, nodes: &[Node]) -> io::Result<()> {
    for (i, node) in nodes.iter().enumerate() {
        if i > 0 {
            write!(w, ",")?;
        }
        entry(w, &tree.packages[node.index])?;
        if let Some(stop) = node.stop {
            write!(w, ",\"stop\":")?;
            report::string(w, stop.id())?;
            if let Stop::Depth(hidden) = stop {
                write!(w, ",\"hidden\":{hidden}")?;
            }
        }
        write!(w, ",\"dependencies\":[")?;
        nodes_json(w, tree, &node.deps)?;
        write!(w, "]}}")?;
    }
    Ok(())
}
