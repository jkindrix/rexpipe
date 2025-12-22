Name:           rexpipe
Version:        2.0.0
Release:        1%{?dist}
Summary:        Modern regex pipeline processor for automated text processing

License:        MIT OR Apache-2.0
URL:            https://github.com/jkindrix/rexpipe
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.85
BuildRequires:  cargo

%description
rexpipe is a powerful command-line tool for processing text using
regex-based pipelines. It supports complex transformations, filtering,
extraction, and validation of text data.

Features:
- Multi-step pipeline processing
- In-place file editing with backup
- Pattern libraries for reusable patterns
- PCRE support for advanced regex features
- Parallel file processing
- Inspect mode for interactive pattern testing

%prep
%autosetup

%build
cargo build --release

%install
install -Dm755 target/release/rexpipe %{buildroot}%{_bindir}/rexpipe

# Generate shell completions
%{buildroot}%{_bindir}/rexpipe --completions bash > %{buildroot}%{_datadir}/bash-completion/completions/rexpipe
%{buildroot}%{_bindir}/rexpipe --completions zsh > %{buildroot}%{_datadir}/zsh/site-functions/_rexpipe
%{buildroot}%{_bindir}/rexpipe --completions fish > %{buildroot}%{_datadir}/fish/vendor_completions.d/rexpipe.fish

# Generate man page
mkdir -p %{buildroot}%{_mandir}/man1
%{buildroot}%{_bindir}/rexpipe --man > %{buildroot}%{_mandir}/man1/rexpipe.1

%check
cargo test --release

%files
%license LICENSE-MIT LICENSE-APACHE
%doc README.md CHANGELOG.md
%{_bindir}/rexpipe
%{_datadir}/bash-completion/completions/rexpipe
%{_datadir}/zsh/site-functions/_rexpipe
%{_datadir}/fish/vendor_completions.d/rexpipe.fish
%{_mandir}/man1/rexpipe.1*

%changelog
* Sun Dec 22 2024 Justin Kindrix <jkindrix@gmail.com> - 2.0.0-1
- Initial package release
