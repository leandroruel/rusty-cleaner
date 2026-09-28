# Publishing rusty-cleaner-bin to the AUR

The AUR package lives in this directory. It repackages the official `.deb`
from GitHub Releases (smaller than the AppImage, and it ships the desktop
entry + icons).

## First-time setup

1. Create an account at <https://aur.archlinux.org> (if you don't have one).
2. Add your **SSH public key** to the account (My Account → Account Actions →
   Edit Info → SSH Public Key). AUR pushes require SSH.
3. Verify SSH auth works:

   ```sh
   ssh aur@aur.archlinux.org help
   ```

## Publish

```sh
git clone ssh://aur@aur.archlinux.org/rusty-cleaner-bin.git
cd rusty-cleaner-bin
cp /path/to/rusty-cleaner/packaging/aur/PKGBUILD .
cp /path/to/rusty-cleaner/packaging/aur/.SRCINFO .
git add PKGBUILD .SRCINFO
git commit -m "Initial import: rusty-cleaner-bin 0.1.0-1"
git push
```

> The AUR git repo has no `main` branch — push to the default (unnamed HEAD)
> with a plain `git push`.

## Update on a new release

1. Bump `pkgver` in `PKGBUILD`, update the `.deb` URL and its `sha256sum`.
2. Regenerate `.SRCINFO` (on an Arch machine): `makepkg --printsrcinfo > .SRCINFO`
3. Commit + push to the AUR repo.

> The LICENSE source points to the `main` branch (the v0.1.0 tag predates the
> LICENSE file). Once a release tag that includes `LICENSE` exists (e.g.
> v0.1.1), switch the URL to that tag and update its checksum.

## Test locally (on Arch)

```sh
cd packaging/aur
makepkg -sf          # builds rusty-cleaner-bin-0.1.0-1-x86_64.pkg.tar.zst
namcap PKGBUILD       # lint
sudo pacman -U rusty-cleaner-bin-0.1.0-1-x86_64.pkg.tar.zst
```
