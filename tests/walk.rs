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
    fn find(&self) -> Vec<String> {
        walk::lockfiles(&self.0, KNOWN)
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
    assert!(walk::lockfiles(&missing, KNOWN).is_empty());
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
