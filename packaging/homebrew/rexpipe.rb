class Rexpipe < Formula
  desc "Modern regex pipeline processor for automated text processing"
  homepage "https://github.com/jkindrix/rexpipe"
  version "2.0.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/jkindrix/rexpipe/releases/download/v#{version}/rexpipe-darwin-aarch64.tar.gz"
      # sha256 will be filled in during release
      sha256 "PLACEHOLDER_SHA256_DARWIN_AARCH64"
    end

    on_intel do
      url "https://github.com/jkindrix/rexpipe/releases/download/v#{version}/rexpipe-darwin-x86_64.tar.gz"
      # sha256 will be filled in during release
      sha256 "PLACEHOLDER_SHA256_DARWIN_X86_64"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/jkindrix/rexpipe/releases/download/v#{version}/rexpipe-linux-aarch64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_AARCH64"
    end

    on_intel do
      url "https://github.com/jkindrix/rexpipe/releases/download/v#{version}/rexpipe-linux-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X86_64"
    end
  end

  def install
    bin.install "rexpipe"

    # Generate shell completions
    generate_completions_from_executable(bin/"rexpipe", "--completions", shells: [:bash, :zsh, :fish])

    # Generate man page
    system bin/"rexpipe", "--man", ">", man1/"rexpipe.1"
  end

  test do
    # Test basic functionality
    assert_match "2.0.0", shell_output("#{bin}/rexpipe --version")

    # Test substitution
    output = pipe_output("#{bin}/rexpipe -e 's/foo/bar/'", "hello foo world")
    assert_match "hello bar world", output

    # Test filter
    output = pipe_output("#{bin}/rexpipe -e 'filter/DEBUG/drop'", "INFO: hello\nDEBUG: debug\nINFO: world")
    assert_no_match(/DEBUG/, output)
    assert_match "hello", output
  end
end
