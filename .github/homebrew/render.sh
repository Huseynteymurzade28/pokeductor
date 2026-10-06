#!/usr/bin/env bash
# Writes the Homebrew formula for one release to stdout.
#
#   render.sh v0.7.0 DIR
#
# DIR holds the release's `.sha256` files, as `gh release download -p
# '*.sha256'` leaves them. The formula installs the prebuilt archives rather
# than building from source: they already exist on every release, and a source
# formula would make each install compile the whole dependency tree.
#
# The Linux builds are the musl ones, since they carry no glibc floor and so
# run on whatever distribution Homebrew is installed on.
set -euo pipefail

tag="$1"
dir="$2"
repo="https://github.com/Huseynteymurzade28/pokeductor"

# The checksum of one target's archive, or a failure naming the missing file,
# so a release without some target cannot produce a formula that points at it.
sha() {
  local file="$dir/pokeductor-$tag-$1.tar.gz.sha256"
  [ -s "$file" ] || { echo "render.sh: no checksum for $1 in $dir" >&2; exit 1; }
  cut -d' ' -f1 "$file"
}

archive() {
  printf '%s/releases/download/%s/pokeductor-%s-%s.tar.gz' "$repo" "$tag" "$tag" "$1"
}

mac_arm=$(sha aarch64-apple-darwin)
mac_intel=$(sha x86_64-apple-darwin)
linux_arm=$(sha aarch64-unknown-linux-musl)
linux_intel=$(sha x86_64-unknown-linux-musl)

cat <<RUBY
# Written by pokeductor's release workflow for $tag. Edits here are
# overwritten by the next release.
class Pokeductor < Formula
  desc "Terminal Pokedex and evolution analyzer"
  homepage "$repo"
  license "MIT"

  on_macos do
    on_arm do
      url "$(archive aarch64-apple-darwin)"
      sha256 "$mac_arm"
    end
    on_intel do
      url "$(archive x86_64-apple-darwin)"
      sha256 "$mac_intel"
    end
  end

  on_linux do
    on_arm do
      url "$(archive aarch64-unknown-linux-musl)"
      sha256 "$linux_arm"
    end
    on_intel do
      url "$(archive x86_64-unknown-linux-musl)"
      sha256 "$linux_intel"
    end
  end

  def install
    bin.install "pokeductor"
    bash_completion.install "completions/pokeductor.bash" => "pokeductor"
    zsh_completion.install "completions/_pokeductor"
    fish_completion.install "completions/pokeductor.fish"
    man1.install "man/pokeductor.1"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/pokeductor --version")
    assert_match "pokeductor", shell_output("#{bin}/pokeductor --cache-dir")
  end
end
RUBY
