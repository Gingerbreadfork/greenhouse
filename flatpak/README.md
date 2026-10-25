# Flatpak

The manifest builds the working tree. Its Flathub copy builds a release tag
instead, and lives in the `flathub/io.github.Gingerbreadfork.Greenhouse`
repository once the app is accepted.

You need `flatpak`, the `org.flatpak.Builder` Flatpak, `git` and `uv`.

## Build and run locally

```bash
flatpak/generate.sh
cd flatpak
flatpak run --filesystem="$(dirname "$PWD")" org.flatpak.Builder --user \
  --install-deps-from=flathub --force-clean --install build-dir \
  io.github.Gingerbreadfork.Greenhouse.yml
flatpak run io.github.Gingerbreadfork.Greenhouse
```

Run `generate.sh` again whenever `Cargo.lock` or `pnpm-lock.yaml` changes.

## Lint

```bash
cd flatpak
flatpak run --filesystem="$(dirname "$PWD")" --command=flatpak-builder-lint \
  org.flatpak.Builder manifest io.github.Gingerbreadfork.Greenhouse.yml
flatpak run --filesystem="$(dirname "$PWD")" --command=flatpak-builder-lint \
  org.flatpak.Builder repo repo
```

The repo check needs a build made with `--repo=repo`.

## Submitting or updating on Flathub

1. Tag and publish the release as usual, with a `<release>` entry for it at
   the top of the metainfo file.
2. Run `flatpak/generate.sh vX.Y.Z`. It writes `flatpak/flathub/` with the
   manifest pinned to that tag and commit, and the sources from that tag's
   lockfiles.
3. For the first submission, fork `flathub/flathub`, branch from `new-pr`,
   copy in the files from `flatpak/flathub/`, add shared-modules with
   `git submodule add https://github.com/flathub/shared-modules.git`, and open
   a pull request against `new-pr`. For later releases, open a pull request
   with the same files against the app's own Flathub repository.

## In the sandbox

- Updates come from Flathub, so the built-in updater is off and Settings
  says so.
- Starting at login goes through the Background portal.
- Downloads can go to the Downloads folder directly. A folder elsewhere is
  granted through the folder picker.
- A finished file opens in the default player. An unfinished one streams to
  the browser, since the sandbox cannot start mpv or VLC itself.
