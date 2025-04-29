{
  description = "Build a cargo project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane = {
      url = "github:ipetkov/crane";
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-analyzer-src.follows = "";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    fenix,
    flake-utils,
    advisory-db,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {inherit system;};

      inherit (pkgs) lib;

      craneLib = crane.mkLib nixpkgs.legacyPackages.${system};
      src = craneLib.cleanCargoSource (craneLib.path ./.);

      # Common arguments can be set here to avoid repeating them later
      commonArgs = {
        inherit src;
        strictDeps = true;

        buildInputs =
          lib.optionals pkgs.stdenv.isDarwin [
            # Additional darwin specific inputs can be set here
            pkgs.libiconv
          ];

        nativeBuildInputs = [
          pkgs.pkg-config
          pkgs.installShellFiles
        ];

        # Additional environment variables can be set directly
        # MY_CUSTOM_VAR = "some value";
      };

      craneLibLLvmTools =
        craneLib.overrideToolchain
        (fenix.packages.${system}.complete.withComponents [
          "cargo"
          "llvm-tools"
          "rustc"
        ]);

      # Build *just* the cargo dependencies, so we can reuse
      # all of that work (e.g. via cachix) when running in CI
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      # Build the actual crate itself, reusing the dependency
      # artifacts from above.
      neorg-task-sync = craneLib.buildPackage (
        commonArgs
        // {
          inherit cargoArtifacts;
          doCheck = false; # TODO: Fix failing tests!

          preBuild = ''
            sed -i \
              -e "s/crate_version!()/\"built via nix from ${self.rev or self.dirtyRev}\"/g" \
              -e "s/crate_version, //g" \
              src/opts.rs
          '';

          postInstall = ''
            installShellCompletion --cmd neorg-task-sync                  \
              --bash <($out/bin/neorg-task-sync generate completion bash) \
              --fish <($out/bin/neorg-task-sync generate completion fish) \
              --zsh  <($out/bin/neorg-task-sync generate completion zsh)
          '';
        }
      );
    in {
      checks = {
        # Build the crate as part of `nix flake check` for convenience
        inherit neorg-task-sync;

        # Run clippy (and deny all warnings) on the crate source,
        # again, reusing the dependency artifacts from above.
        #
        # Note that this is done as a separate derivation so that
        # we can block the CI if there are issues here, but not
        # prevent downstream consumers from building our crate by itself.
        neorg-task-sync-clippy = craneLib.cargoClippy (commonArgs
          // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

        neorg-task-sync-doc = craneLib.cargoDoc (commonArgs
          // {
            inherit cargoArtifacts;
          });

        # Check formatting
        neorg-task-sync-fmt = craneLib.cargoFmt {
          inherit src;
        };

        # Audit dependencies
        neorg-task-sync-audit = craneLib.cargoAudit {
          inherit src advisory-db;
        };

        # Audit licenses
        # neorg-task-sync-deny = craneLib.cargoDeny {
        # inherit src;
        # };

        # Run tests with cargo-nextest
        # Consider setting `doCheck = false` on `neorg-task-sync` if you do not want
        # the tests to run twice
        neorg-task-sync-nextest = craneLib.cargoNextest (commonArgs
          // {
            inherit cargoArtifacts;
            partitions = 1;
            partitionType = "count";
          });
      };

      packages =
        {
          default = neorg-task-sync;
        }
        // lib.optionalAttrs (!pkgs.stdenv.isDarwin) {
          neorg-task-sync-llvm-coverage = craneLibLLvmTools.cargoLlvmCov (commonArgs
            // {
              inherit cargoArtifacts;
            });
        };

      apps.default = flake-utils.lib.mkApp {
        drv = neorg-task-sync;
      };

      devShells.default = craneLib.devShell {
        # Inherit inputs from checks.
        checks = self.checks.${system};

        # Additional dev-shell environment variables can be set directly
        # MY_CUSTOM_DEVELOPMENT_VAR = "something else";

        # Extra inputs can be added here; cargo and rustc are provided by default.
        packages = with pkgs; [
          bacon
          rust-analyzer
          # pkgs.ripgrep
        ];
      };
    });
}
