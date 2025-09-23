{ ... }:
{
  perSystem =
    { pkgs, ... }:
    let
      rust = pkgs.fenix.stable;
      rustToolchain = pkgs.fenix.combine [
        (rust.withComponents [
          "cargo"
          "clippy"
          "rustc"
          "rustfmt"
          "rust-src"
          "rust-analyzer"
        ])
      ];

      envs = {
        rust = {
          RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc; # Required for rust-analyzer

          # Force system OpenSSL instead of vendored version
          # Reference: https://docs.rs/openssl/latest/openssl/#manual
          OPENSSL_NO_VENDOR = 1;
          OPENSSL_LIB_DIR = "${pkgs.lib.getLib pkgs.openssl}/lib";
          OPENSSL_DIR = "${pkgs.lib.getDev pkgs.openssl}";
        };

        clang = {
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        };
      };

      # PATH extensions for package managers
      pathExtensions = {
        cargo = ''export PATH="$PATH:~/.cargo/bin"'';
      };

      # Diagnostic information shown when shell starts
      devInfo = ''
        echo "RUSTC version: $(rustc --version)"
        echo "CARGO version: $(cargo --version)"
        echo "Build directory: $PWD"
        echo "Source directory: $src"
      '';
    in
    {
      devShells = {
        default = pkgs.mkShell {
          packages = with pkgs; [
            nixd
            natscli
            nats-server
            google-cloud-sdk
            cargo-make
            pkg-config
            opentofu
            rustToolchain
            vscode-extensions.vadimcn.vscode-lldb
          ];

          inherit (envs.rust)
            RUST_SRC_PATH OPENSSL_NO_VENDOR OPENSSL_LIB_DIR OPENSSL_DIR;
          inherit (envs.clang) LIBCLANG_PATH;
          shellHook = ''
            ${pathExtensions.cargo}
            ${devInfo}
          '';
        };
      };
    };
}
