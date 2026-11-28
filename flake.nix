{
  description = "Callcraft: open scenario format and reference simulator for 911 dispatcher training";

  # Pinned to a commit of burrow's main, so Renovate moves it with the pins
  # in .github/workflows, in one PR (burrow's
  # modules/updates/renovate-policy.nix says when); `nix flake update`
  # leaves it where it is.
  inputs.burrow.url = "git+https://github.com/FieldmouseWorks/burrow?ref=main&rev=5fe4bb5868149b51bdb3cf2a555eb297ab41951a";

  # `nix develop` for the toolchains, `nix flake check` for the gate (see
  # burrow's lib/project.nix). The Tauri shell (crates/callcraft-app) links
  # the GTK and WebKitGTK stack on Linux.
  outputs = { self, burrow }: burrow.lib.project {
    inherit self;
    rust = {
      nativeBuildInputs = pkgs: [ pkgs.pkg-config ];
      buildInputs = pkgs: [
        pkgs.webkitgtk_4_1
        pkgs.gtk3
        pkgs.libsoup_3
        pkgs.glib
        pkgs.openssl
      ];
      # The server builds its provider HTTP client at start, and rustls'
      # platform verifier wants the system's CA roots, which the Nix sandbox
      # doesn't have; cacert's setup hook points SSL_CERT_FILE at its bundle.
      testInputs = pkgs: [ pkgs.cacert ];
    };
  };
}
