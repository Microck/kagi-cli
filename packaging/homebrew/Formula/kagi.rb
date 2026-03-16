class Kagi < Formula
  desc "Agent-native Rust CLI for Kagi subscribers with JSON-first output"
  homepage "https://github.com/Microck/kagi-cli"
  version "0.1.5"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/Microck/kagi-cli/releases/download/v0.1.5/kagi-v0.1.5-aarch64-apple-darwin.tar.gz"
      sha256 "7de432257ef363c4202418325dcb8dbcf4aad00cc5bc799b404fad2912081176"
    end

    if Hardware::CPU.intel?
      url "https://github.com/Microck/kagi-cli/releases/download/v0.1.5/kagi-v0.1.5-x86_64-apple-darwin.tar.gz"
      sha256 "6799839cf956c58cbca8c4f4a68999ba023da4e799ed296e87aa28f782355a5f"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/Microck/kagi-cli/releases/download/v0.1.5/kagi-v0.1.5-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "063d96531ea32b1525e15b8f0fdfccabb2ea2c3a5a2b30105c5ff2e1e1c85fd7"
    end

    if Hardware::CPU.intel?
      url "https://github.com/Microck/kagi-cli/releases/download/v0.1.5/kagi-v0.1.5-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "b06d89788098ae92030cf6e80de2a7832ec594f96f8a882be03dea56025f965a"
    end
  end

  def install
    bin.install "kagi"
  end

  test do
    assert_match "Usage: kagi <COMMAND>", shell_output("#{bin}/kagi --help")
  end
end
