{
  description = "Spacebot - An AI agent for teams, communities, and multi-user environments";

  inputs = {
    # Pinned to a specific nixpkgs rev so `nix flake update` is an explicit act.
    # This rev ships bun 1.3.11, which matches the project-local mise pin and
    # the `packageManager` field in spaceui/package.json. Update all three
    # together when bumping. See CLAUDE.md "Package Managers" section.
    nixpkgs.url = "github:NixOS/nixpkgs/566acc07c54dc807f91625bb286cb9b321b5f42a";
    flake-utils.url = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    crane,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {
          inherit system;
        };

        inherit (pkgs) bun;

        craneLib = crane.mkLib pkgs;

        cargoSrc = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./build.rs
            ./src
            ./migrations
            ./prompts
            ./presets
            ./docs/content
            ./docs/README.md
            ./docs/docker.md
            ./docs/metrics.md
            ./AGENTS.md
            ./README.md
            ./CHANGELOG.md
            ./.cargo/config.toml
            (pkgs.lib.fileset.maybeMissing ./interface/dist)
            ./vendor
          ];
        };

        runtimeAssetsSrc = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            ./migrations
            ./prompts
          ];
        };

        frontendSrc = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            ./interface/package.json
            ./interface/bun.lock
            ./interface/index.html
            ./interface/tsconfig.json
            ./interface/tsconfig.node.json
            ./interface/vite.config.ts
            ./interface/public
            ./interface/src
            ./spaceui/packages/tokens/src
            ./spaceui/packages/tokens/package.json
            ./spaceui/packages/tokens/raw-colors.cjs
            ./spaceui/packages/tokens/raw-colors.d.ts
            ./spaceui/packages/tokens/tsconfig.json
            ./spaceui/packages/primitives/src
            ./spaceui/packages/primitives/package.json
            ./spaceui/packages/primitives/tsconfig.json
            ./spaceui/packages/primitives/tsup.config.ts
            ./spaceui/packages/ai/src
            ./spaceui/packages/ai/package.json
            ./spaceui/packages/ai/tsconfig.json
            ./spaceui/packages/ai/tsup.config.ts
            ./spaceui/packages/forms/src
            ./spaceui/packages/forms/package.json
            ./spaceui/packages/forms/tsconfig.json
            ./spaceui/packages/forms/tsup.config.ts
            ./spaceui/packages/explorer/src
            ./spaceui/packages/explorer/package.json
            ./spaceui/packages/explorer/tsconfig.json
            ./spaceui/packages/explorer/tsup.config.ts
            ./spaceui/packages/icons
            ./spaceui/package.json
            ./spaceui/bun.lock
            ./spaceui/turbo.json
            ./spaceui/tsconfig.base.json
            # Workspace members listed in spaceui/package.json — bun requires
            # each to exist on disk when running install. Only package.json
            # is needed for workspace discovery; node_modules and build
            # artifacts stay excluded.
            ./spaceui/.storybook/package.json
            ./spaceui/examples/showcase/package.json
          ];
        };

        spacebotPackages = import ./nix {
          inherit pkgs craneLib cargoSrc runtimeAssetsSrc frontendSrc;
        };

        inherit (spacebotPackages) frontend frontendNodeModules spacebot spacebot-full spacebot-tests;
      in {
        packages = {
          default = spacebot;
          inherit frontend spacebot spacebot-full;
          # Updater for frontend deps - run this to get the correct hash after updating interface deps
          # Usage: nix build .#frontend-updater 2>&1 | grep "got:" | awk '{print $2}'
          frontend-updater = frontendNodeModules { hash = pkgs.lib.fakeHash; };
        };

        devShells = {
          default = pkgs.mkShell {
            packages = with pkgs; [
              rustc
              cargo
              rustfmt
              rust-analyzer
              clippy
              bun
              nodejs
              protobuf
              cmake
              openssl
              pkg-config
              onnxruntime
            ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.chromium ];

            ORT_LIB_LOCATION = "${pkgs.onnxruntime}/lib";
            CHROME_PATH = if pkgs.stdenv.isLinux then "${pkgs.chromium}/bin/chromium" else "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
            CHROME_FLAGS = "--no-sandbox --disable-dev-shm-usage --disable-gpu";
          };

          backend = pkgs.mkShell {
            packages = with pkgs; [
              rustc
              cargo
              rustfmt
              rust-analyzer
              clippy
              protobuf
              cmake
              openssl
              pkg-config
              onnxruntime
            ];

            ORT_LIB_LOCATION = "${pkgs.onnxruntime}/lib";
          };
        };

        checks = {
          inherit spacebot spacebot-full spacebot-tests;
        };
      }
    )
    // {
      overlays.default = final: {
        inherit (self.packages.${final.system}) spacebot spacebot-full;
      };

      nixosModules.default = import ./nix/module.nix self;
    };
}
