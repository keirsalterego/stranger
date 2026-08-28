# Install scripts

`npm install` runs a dependency's `preinstall`, `install` and `postinstall` hooks
as part of installing it. Before your test suite, before your own first line of
code, with your environment and whatever your ssh agent is holding.

That is the whole argument for `high`. For these packages the gap between "a name
appeared in the lockfile" and "that name's code ran on this machine" is one
command, and no review step fits inside it.

```console
$ ./target/release/stranger scan -v fixtures/npm-m.package-lock.json

  npm-m.package-lock.json  576 packages   (20 direct · 556 transitive · 6 workspace)

  ⚠  INSTALL SCRIPTS        4     arbitrary code at install time
     esbuild@0.27.7                         runs code at install time · lockfile records the flag, not the script
     fsevents@2.3.3                         runs code at install time · lockfile records the flag, not the script
     sharp@0.34.5                           runs code at install time · lockfile records the flag, not the script
     unrs-resolver@1.12.2                   runs code at install time · lockfile records the flag, not the script
```

## The signal

One field:

```console
$ jq '.packages["node_modules/esbuild"] | {version, hasInstallScript}' fixtures/npm-xl.package-lock.json
{
  "version": "0.28.1",
  "hasInstallScript": true
}
```

That is everything lockfileVersion 3 records. Not the body of the script, not
which of the three hooks, not even the script's name. The body is in the tarball
on the registry, and `stranger` does not fetch.

## What it cannot see

`esbuild` unpacking a platform binary and a package curling a payload produce the
identical line in this report. Reading that line as triage is the mistake the
rule's own source comment exists to prevent, and the `detail` string is worded so
it never implies otherwise: *runs code at install time · lockfile records the
flag, not the script.*

A finding here is a list of packages to look at, ordered by nothing. It is not a
verdict on any of them.

## What is excluded

The root project's own `hasInstallScript` is your build, not a stranger's. So are
a workspace member's and its `link: true` symlink. All three are dropped.

`jq` counts 9 flagged entries in `npm-xl`; the reader reports 8. The missing one
is the root entry, deliberately.

## Copies count separately

One finding per lockfile entry, not per name. Two entries for one name are two
installs, and two installs run the hook twice, so both are reported. `fsevents`
appears twice in `npm-xl` — once at the top level and once under a workspace
member — and `tests/rules.rs` asserts both hits are there:

```console
$ jq -r '.packages | to_entries[] | select(.key | endswith("node_modules/fsevents")) | "\(.key)  \(.value.version)"' fixtures/npm-xl.package-lock.json
apps/desktop/node_modules/fsevents  2.3.2
node_modules/fsevents  2.3.3
```

Both are reported:

```console
$ ./target/release/stranger scan -v fixtures/npm-xl.package-lock.json | head -12

  npm-xl.package-lock.json 1,376 packages   (150 direct · 1,226 transitive · 14 workspace)

  ⚠  INSTALL SCRIPTS        8     arbitrary code at install time
     agent-browser@0.26.0                   runs code at install time · lockfile records the flag, not the script
     electron@40.10.2                       runs code at install time · lockfile records the flag, not the script
     electron-winstaller@5.4.0              runs code at install time · lockfile records the flag, not the script
     esbuild@0.28.1                         runs code at install time · lockfile records the flag, not the script
     fsevents@2.3.2                         runs code at install time · lockfile records the flag, not the script
     fsevents@2.3.3                         runs code at install time · lockfile records the flag, not the script
     node-pty@1.1.0                         runs code at install time · lockfile records the flag, not the script
     unicode-animations@1.0.3               runs code at install time · lockfile records the flag, not the script
```

Version is part of the sort key, so the order is stable across scans.

## It fires on npm only

`requirements.txt` records nothing equivalent. A pip source distribution can run
whatever `setup.py` wants during installation and the file does not say that it
will, which is a real blind spot rather than a rule that does not apply — see
[Limits](../limits.md).

## Reading a hit

The poisoned fixture shows why this rule is worth having next to the hallucination
rule rather than instead of it:

```console
$ ./target/release/stranger scan --format json fixtures/poisoned.package-lock.json | jq -r '.findings[] | select(.rule=="install-script") | .package'
lodahs
sharp
unrs-resolver
```

`lodahs` appears under two rules that mean different things. One says the name has
no evidence behind it. The other says that if you install it, it runs code. Both
are true and neither implies the other.

```console
$ ./target/release/stranger scan -v fixtures/npm-m.package-lock.json
```
