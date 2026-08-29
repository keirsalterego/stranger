use std::fs;
use std::path::PathBuf;
use stranger::walk;

const KNOWN: &[&str] = &["package-lock.json", "requirements.txt"];

/// Builds a throwaway tree under the target directory so the test does not
/// depend on anything outside the repo and cleans up after itself.
struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Tree {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Tree(root)
    }
    fn file(&self, rel: &str) -> &Tree {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, "{}").unwrap();
        self
    }
    fn dir(&self, rel: &str) -> &Tree {
        fs::create_dir_all(self.0.join(rel)).unwrap();
        self
    }
    /// `target` is written into the link verbatim, so a test can plant the
    /// relative `..`-shaped links that occur in the wild as easily as an
    /// absolute one.
    #[cfg(unix)]
    fn link(&self, target: &str, rel: &str) -> &Tree {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, p).unwrap();
        self
    }
    fn walk(&self) -> walk::Walk {
        walk::lockfiles(&self.0, KNOWN)
    }
    fn find(&self) -> Vec<String> {
        self.walk()
            .found
            .into_iter()
            .map(|p| {
                p.strip_prefix(&self.0)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }
}

/// chmod 000 does nothing to root, and a test that passes because its assertion
/// could not fail is worse than no test. std has no `geteuid` and this crate
/// forbids the `unsafe` an FFI one would need, so ask the filesystem instead:
/// take the permissions away and see whether they went.
#[cfg(unix)]
fn lock_out(dir: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o000)).expect("chmod");
    fs::read_dir(dir).is_err()
}

#[cfg(unix)]
fn unlock(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o755));
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn finds_nested_manifests() {
    let t = Tree::new("walk_nested");
    t.file("package-lock.json")
        .file("apps/web/package-lock.json")
        .file("services/api/requirements.txt");
    assert_eq!(
        t.find(),
        vec![
            "apps/web/package-lock.json",
            "package-lock.json",
            "services/api/requirements.txt"
        ]
    );
}

/// The one that matters. A populated `node_modules` holds hundreds of other
/// people's lockfiles, and walking into it turns one scan into four hundred
/// irrelevant ones.
#[test]
fn never_descends_into_node_modules() {
    let t = Tree::new("walk_node_modules");
    t.file("package-lock.json")
        .file("node_modules/left-pad/package-lock.json")
        .file("node_modules/.bin/package-lock.json")
        .file("apps/web/node_modules/x/package-lock.json");
    assert_eq!(t.find(), vec!["package-lock.json"]);
}

#[test]
fn skips_build_output_and_dotdirs() {
    let t = Tree::new("walk_skips");
    t.file("package-lock.json")
        .file("target/debug/package-lock.json")
        .file(".git/package-lock.json")
        .file(".venv/lib/requirements.txt")
        .file("dist/package-lock.json")
        .file("__pycache__/requirements.txt");
    assert_eq!(t.find(), vec!["package-lock.json"]);
}

/// `read_dir` hands back filesystem order, which on ext4 is hash order. Two
/// runs reporting the same findings in a different sequence makes a diff
/// between scans mostly noise.
#[test]
fn order_is_stable() {
    let t = Tree::new("walk_order");
    for n in ["z", "a", "m", "b"] {
        t.file(&format!("{n}/package-lock.json"));
    }
    let first = t.find();
    assert_eq!(
        first,
        vec![
            "a/package-lock.json",
            "b/package-lock.json",
            "m/package-lock.json",
            "z/package-lock.json"
        ]
    );
    assert_eq!(t.find(), first);
}

#[test]
fn depth_is_bounded() {
    let t = Tree::new("walk_depth");
    let deep: String = (0..walk::MAX_DEPTH + 3)
        .map(|i| format!("d{i}/"))
        .collect::<String>()
        + "package-lock.json";
    t.file(&deep).file("shallow/package-lock.json");
    assert_eq!(t.find(), vec!["shallow/package-lock.json"]);
}

#[test]
fn an_empty_tree_finds_nothing() {
    let t = Tree::new("walk_empty");
    t.dir("src").dir("docs");
    assert!(t.find().is_empty());
}

#[test]
fn a_missing_root_is_not_a_panic() {
    let missing = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("definitely-not-here");
    let w = walk::lockfiles(&missing, KNOWN);
    assert!(w.found.is_empty());
    // A directory that is not there hides nothing, so it is not a blind spot.
    // Counting it as one would turn every `scan /typo` into an incomplete scan
    // instead of a path that does not exist.
    assert!(w.unreadable.is_empty());
}

/// The one that decides an exit code. A `read_dir` failure came back as an
/// empty vec, indistinguishable from an empty directory, and a tree whose only
/// lockfile sat behind a 000 directory scanned clean.
#[cfg(unix)]
#[test]
fn an_unreadable_directory_comes_back_named() {
    let t = Tree::new("walk_unreadable");
    t.file("package-lock.json").file("locked/package-lock.json");
    let locked = t.0.join("locked");
    if !lock_out(&locked) {
        return; // running as root, which can read anything
    }
    let w = t.walk();
    // Restored before the assertions, or a failing run leaves behind a
    // directory `Drop` cannot remove.
    unlock(&locked);

    assert_eq!(w.found.len(), 1, "{:?}", w.found);
    assert_eq!(w.unreadable, vec![locked]);
}

/// Eight lockfiles in a directory used to produce "no lockfile". Not reading
/// yarn is a declared cut; telling somebody holding a `yarn.lock` that they
/// have no lockfile is a wrong answer.
#[test]
fn a_lockfile_with_no_reader_is_still_named() {
    let t = Tree::new("walk_unsupported");
    t.file("yarn.lock")
        .file("go.sum")
        .file("pkg/Gemfile.lock")
        .file("notes.json");
    let w = t.walk();
    assert!(w.found.is_empty(), "{:?}", w.found);
    assert_eq!(w.unsupported, vec!["Gemfile.lock", "go.sum", "yarn.lock"]);
}

/// Deduped, or a monorepo with one `yarn.lock` per package names it ten times.
#[test]
fn one_unsupported_name_however_many_copies() {
    let t = Tree::new("walk_unsupported_dedup");
    t.file("a/yarn.lock")
        .file("b/yarn.lock")
        .file("c/yarn.lock");
    assert_eq!(t.walk().unsupported, vec!["yarn.lock"]);
}

/// `go.mod` has a reader now. It sat on the unsupported list while it did not,
/// and a name on both lists would be found *and* reported as unread.
#[test]
fn a_format_with_a_reader_is_never_called_unsupported() {
    let t = Tree::new("walk_gomod_supported");
    t.file("go.mod").file("go.sum");
    let w = walk::lockfiles(&t.0, stranger::lock::KNOWN);
    assert_eq!(w.found.len(), 1, "{:?}", w.found);
    assert_eq!(w.unsupported, vec!["go.sum"]);
}

/// Every directory passed over on purpose is recorded with why, because a blind
/// spot nobody is told about looks exactly like a clean tree. `dist/`, `.ci/`
/// and anything below `MAX_DEPTH` were all invisible.
#[test]
fn skipped_directories_carry_their_reason() {
    let t = Tree::new("walk_skipped");
    t.dir("node_modules").dir(".ci").dir("dist");
    let deep: String = (0..=walk::MAX_DEPTH).map(|i| format!("d{i}/")).collect();
    t.dir(&deep);

    let w = t.walk();
    let why = |name: &str| {
        w.skipped
            .iter()
            .find(|(p, _)| p.ends_with(name))
            .map(|(_, why)| *why)
    };
    assert_eq!(why("node_modules"), Some("on the skip list"));
    assert_eq!(why("dist"), Some("on the skip list"));
    assert_eq!(why(".ci"), Some("hidden"));
    assert_eq!(
        why(&format!("d{}", walk::MAX_DEPTH)),
        Some("below MAX_DEPTH")
    );
}

/// The depth cut is real and the number is the one documented: a lockfile at
/// `MAX_DEPTH` is found and its sibling one level further down is not.
#[test]
fn max_depth_is_where_it_says_it_is() {
    let t = Tree::new("walk_depth");
    let at = |n: usize| (0..n).map(|i| format!("d{i}/")).collect::<String>();
    t.file(&format!("{}package-lock.json", at(walk::MAX_DEPTH)));
    t.file(&format!("{}package-lock.json", at(walk::MAX_DEPTH + 1)));
    assert_eq!(t.find().len(), 1, "{:?}", t.find());
}

/// A monorepo that keeps one lockfile and links it from every package used to
/// scan as empty: the skip was written for directory cycles but caught every
/// symlink, files included.
#[cfg(unix)]
#[test]
fn follows_a_symlinked_lockfile() {
    let t = Tree::new("walk_symlink_file");
    t.file("upstream/shared.json");
    t.link("../upstream/shared.json", "pkg/package-lock.json");
    assert_eq!(t.find(), vec!["pkg/package-lock.json"]);
}

/// The other half of the same change. Directory symlinks are what make a walk
/// infinite, so they stay skipped — a link back to the root and a `..`-shaped
/// link of the kind `node_modules` is full of, both dead ends.
#[cfg(unix)]
#[test]
fn a_symlinked_directory_cycle_terminates() {
    let t = Tree::new("walk_symlink_cycle");
    let root = t.0.to_string_lossy().into_owned();
    t.file("a/package-lock.json");
    t.link(&root, "a/loop");
    t.link("..", "a/b/up");
    assert_eq!(t.find(), vec!["a/package-lock.json"]);
}

/// A link with nothing on the end of it is neither a file to read nor a
/// directory to descend, and must not abort the rest of the scan.
#[cfg(unix)]
#[test]
fn a_dangling_symlink_is_skipped() {
    let t = Tree::new("walk_symlink_dangling");
    t.file("package-lock.json");
    t.link("./nothing-here.json", "pkg/package-lock.json");
    assert_eq!(t.find(), vec!["package-lock.json"]);
}

/// Fixtures are named `npm-xl.package-lock.json`, so matching is by suffix.
#[test]
fn matches_prefixed_fixture_names() {
    let t = Tree::new("walk_prefixed");
    t.file("npm-xl.package-lock.json")
        .file("reqs-s.requirements.txt")
        .file("notes.json");
    assert_eq!(
        t.find(),
        vec!["npm-xl.package-lock.json", "reqs-s.requirements.txt"]
    );
}
