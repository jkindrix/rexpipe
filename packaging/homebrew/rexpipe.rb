class Rexpipe < Formula
  desc "Modern regex pipeline processor for automated text processing"
  homepage "https://github.com/jkindrix/rexpipe"
  version "2.0.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/jkindrix/rexpipe/releases/download/v#{version}/rexpipe-darwin-aarch64.tar.gz"
      # SHA256 checksums are updated automatically by the release workflow.
      # To update manually, run: shasum -a 256 <archive>.tar.gz
      sha256 "PLACEHOLDER_SHA256_DARWIN_AARCH64"
    end

    on_intel do
      url "https://github.com/jkindrix/rexpipe/releases/download/v#{version}/rexpipe-darwin-x86_64.tar.gz"
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

    # Generate and install man page
    # rexpipe --man outputs man page content to stdout
    (man1/"rexpipe.1").write Utils.safe_popen_read(bin/"rexpipe", "--man")
  end

  test do
    # Test basic functionality - version output
    assert_match(/rexpipe 2\.\d+\.\d+/, shell_output("#{bin}/rexpipe --version"))

    # Test substitution: -p for pattern, -r for replacement
    output = pipe_output("#{bin}/rexpipe -p 'foo' -r 'bar' --text", "hello foo world\n")
    assert_match "hello bar world", output

    # Test using a config file for filter
    (testpath/"filter.toml").write <<~EOS
      [[step]]
      type = "filter"
      pattern = "DEBUG"
      action = "drop_line"
    EOS

    output = pipe_output("#{bin}/rexpipe -c #{testpath}/filter.toml --text", "INFO: hello\nDEBUG: debug\nINFO: world\n")
    refute_match(/DEBUG/, output)
    assert_match "INFO: hello", output
  end
end
