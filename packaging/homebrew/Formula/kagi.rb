class Kagi < Formula
  desc "Agent-native Rust CLI for Kagi subscribers with JSON-first output"
  homepage "https://github.com/Microck/kagi-cli"
  version "0.9.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/Microck/kagi-cli/releases/download/v0.9.0/kagi-v0.9.0-aarch64-apple-darwin.tar.gz"
      sha256 "d5d4daf70a730d7d98f340927c6cf12e109c4e910564469593bef50d12f553f5"
    end

    if Hardware::CPU.intel?
      url "https://github.com/Microck/kagi-cli/releases/download/v0.9.0/kagi-v0.9.0-x86_64-apple-darwin.tar.gz"
      sha256 "f7d7ecb04d13bdf6be33cb4f1a88a50d2139e8e1123d8f388c4d89c9a9d9b966"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/Microck/kagi-cli/releases/download/v0.9.0/kagi-v0.9.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "a22fde549b32402e062bb46aa43ce968ec7d4432ff7a1ee37e4d45ce739b1cff"
    end

    if Hardware::CPU.intel?
      url "https://github.com/Microck/kagi-cli/releases/download/v0.9.0/kagi-v0.9.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "09bd758be6f374384aa73c1d6d22e92bcfced40537d06ea832dfb80a659dea02"
    end
  end

  def install
    bin.install "kagi"
  end

  test do
    assert_match "Usage: kagi [OPTIONS] [COMMAND]", shell_output("#{bin}/kagi --help")
  end
end
