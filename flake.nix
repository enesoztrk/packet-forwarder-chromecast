{
  description = "Rust development dev shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    crane,
    rust-overlay,
    advisory-db,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [(import rust-overlay)];
        };

        inherit (pkgs) lib;
        rustTarget = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        
 

        craneLib = crane.mkLib pkgs;
        src = craneLib.cleanCargoSource ./.;
        commonArgs = {
          inherit src;
          strictDeps = true;

          buildInputs = with pkgs; [
            openssl
            pkg-config
            eza
            fd
            lldb
            clang
            cargo-audit
            cargo-tarpaulin
          ];
        };

         individualCrateArgs =
          commonArgs
          // {
            inherit cargoArtifacts;
            inherit
              (craneLib.crateNameFromCargoToml {inherit src;})
              version
              ;
            doCheck = false;
            cargoVendorDir = craneLib.vendorMultipleCargoDeps {
              inherit (craneLib.findCargoFiles src) cargoConfigs;
              cargoLockList = [
                ./Cargo.lock
        
              ];
            };
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      fileSetForCrate = crate:
          lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
               ./src
              crate
            ];
          };

        # Sequential flake checking can be utilized for CI/CD purposes.
        # Run squence cmd: 'nix flake check'
        # 1. Check formatting
        pcktFwdPackage-cargoFmt = craneLib.cargoFmt (individualCrateArgs
          // {
            inherit src cargoArtifacts;
          });

        #  2. Run clippy (and deny all warnings) on the crate source.
        pcktFwdPackage-cargoClippy = craneLib.cargoClippy (individualCrateArgs
          // {
            # Again we apply some extra arguments only to this derivation
            # and not every where else. In this case we add some clippy flags
            cargoArtifacts = pcktFwdPackage-cargoFmt;
            nativeBuildInputs = with pkgs; [
            ];
            preBuild = ''
              cargo build --release
            '';
            cargoClippyExtraArgs = "-- --deny warnings";
          });

        # 3. we want to run the tests and collect code-coverage, _but only if
        # the clippy checks pass_ so we do not waste any extra cycles.
        pcktFwdPackage-cargoTarpaulin = craneLib.cargoTarpaulin (individualCrateArgs
          // {
            cargoArtifacts = pcktFwdPackage-cargoClippy;
          });

        # 4. cargo-audit
        pcktFwdPackage-cargoAudit = craneLib.cargoAudit (individualCrateArgs
          // {
            inherit advisory-db;
            cargoArtifacts = pcktFwdPackage-cargoTarpaulin;
          });

        mkpcktFwdPackage = buildType:
          craneLib.buildPackage (individualCrateArgs
            // {
              pname = "pckt-fwd";
              cargoExtraArgs = "";
              src =  fileSetForCrate ./.;
              #CARGO_BUILD_RUSTFLAGS = "-C link-arg=-lasan -Zproc-macro-backtrace";
              nativeBuildInputs = with pkgs; [
                openssl
                pkg-config
                eza
                fd
                lldb
                clang
                cargo-audit
              ];
              buildPhaseCargoCommand = ''
                if [[ "${buildType}" == "release" ]]; then
                     cargo build --release
                  else
                     cargo build 
                  fi

              '';
              installPhase = ''
                mkdir -p $out/bin
                install -D -m755 target/${buildType}/pckt-fwd $out/bin/${buildType}/pckt-fwd
              '';
            });
        # Create packages for different build types
        pcktFwdRelease = mkpcktFwdPackage "release";
        pcktFwdDebug = mkpcktFwdPackage "debug";
      in
        with pkgs; {
          formatter = pkgs.alejandra;
          packages = {
            inherit pcktFwdRelease pcktFwdDebug;
            default = pcktFwdRelease; # Default to release build
          };
          checks = {
            inherit
              # Build the crate as part of `nix flake check` for convenience
              pcktFwdRelease
              pcktFwdPackage-cargoAudit
              ;
          };
          devShells.default = craneLib.devShell {
            # Inherit inputs from checks.
            checks = self.checks.${system};
            inherit (commonArgs) buildInputs;
          };
        }
    );
}