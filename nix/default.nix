{
  pkgs,
  craneLib,
  cargoSrc,
  runtimeAssetsSrc,
  frontendSrc,
}: let
  inherit (pkgs) lib onnxruntime stdenv;

  bunInstallOs =
    if stdenv.buildPlatform.isDarwin
    then "darwin"
    else if stdenv.buildPlatform.isLinux
    then "linux"
    else throw "Unsupported build platform for frontend Bun install: ${stdenv.buildPlatform.system}";

  bunInstallCpu =
    if stdenv.buildPlatform.isAarch64
    then "arm64"
    else if stdenv.buildPlatform.isx86_64
    then "x64"
    else throw "Unsupported build CPU for frontend Bun install: ${stdenv.buildPlatform.system}";

  rollupNativePackage =
    if stdenv.buildPlatform.isLinux && stdenv.buildPlatform.isx86_64
    then "@rollup/rollup-linux-x64-gnu"
    else if stdenv.buildPlatform.isLinux && stdenv.buildPlatform.isAarch64
    then "@rollup/rollup-linux-arm64-gnu"
    else if stdenv.buildPlatform.isDarwin && stdenv.buildPlatform.isx86_64
    then "@rollup/rollup-darwin-x64"
    else if stdenv.buildPlatform.isDarwin && stdenv.buildPlatform.isAarch64
    then "@rollup/rollup-darwin-arm64"
    else null;

  esbuildNativePackage =
    if stdenv.buildPlatform.isLinux && stdenv.buildPlatform.isx86_64
    then "@esbuild/linux-x64"
    else if stdenv.buildPlatform.isLinux && stdenv.buildPlatform.isAarch64
    then "@esbuild/linux-arm64"
    else if stdenv.buildPlatform.isDarwin && stdenv.buildPlatform.isx86_64
    then "@esbuild/darwin-x64"
    else if stdenv.buildPlatform.isDarwin && stdenv.buildPlatform.isAarch64
    then "@esbuild/darwin-arm64"
    else null;

  # Read version from Cargo.toml
  cargoToml = fromTOML (builtins.readFile "${cargoSrc}/Cargo.toml");
  inherit (cargoToml.package) version;

  buildInputs = with pkgs; [
    protobuf
    cmake
    openssl
    pkg-config
    onnxruntime
  ];

  nativeBuildInputs = with pkgs; [
    pkg-config
    protobuf
    cmake
  ] ++ lib.optionals stdenv.isLinux [pkgs.mold];

  frontendNodeModules = {
    hash ? "sha256-dnaECHhGL+Uzu2qlUc34nKgo2SSzWaRm/lYc2e9YlR0=",
  }:
    stdenv.mkDerivation {
      pname = "spacebot-frontend-node-modules";
      inherit version;
      # The whole frontendSrc is in scope because interface/package.json
      # declares `"workspaces": ["../spaceui/packages/*"]` — bun walks
      # sideways to resolve the `@spacedrive/*` workspace targets during
      # install. Without the spaceui packages present, install fails.
      src = frontendSrc;

      nativeBuildInputs = with pkgs; [
        bun
        nodejs
        writableTmpDirAsHomeHook
      ];

      dontConfigure = true;
      dontFixup = true;

      buildPhase = ''
        runHook preBuild

        export BUN_INSTALL_CACHE_DIR="$(mktemp -d)"

        # spaceui holds its own bun workspace; run install there first so
        # each package has its own node_modules resolved (Radix, framer-motion,
        # and so on land inside spaceui/packages/*/node_modules under the
        # isolated linker).
        (
          cd spaceui
          bun install \
            --frozen-lockfile \
            --ignore-scripts \
            --no-progress \
            --os=${bunInstallOs} \
            --cpu=${bunInstallCpu}
        )

        # interface/ references spaceui via workspace symlinks. A fresh
        # install in interface/ places `@spacedrive/*` symlinks into
        # interface/node_modules.
        (
          cd interface
          bun install \
            --frozen-lockfile \
            --ignore-scripts \
            --no-progress \
            --os=${bunInstallOs} \
            --cpu=${bunInstallCpu}
        )

        esbuild_native_package="${if esbuildNativePackage == null then "" else esbuildNativePackage}"
        if [ -n "$esbuild_native_package" ] && [ -f ./interface/node_modules/esbuild/package.json ]; then
          esbuild_version="$(node -p "require('./interface/node_modules/esbuild/package.json').version")"
          (cd interface && bun add --dev --no-save --no-progress "$esbuild_native_package@$esbuild_version")
        fi

        rollup_native_package="${if rollupNativePackage == null then "" else rollupNativePackage}"
        if [ -n "$rollup_native_package" ] && [ -f ./interface/node_modules/rollup/package.json ]; then
          rollup_version="$(node -p "require('./interface/node_modules/rollup/package.json').version")"
          (cd interface && bun add --dev --no-save --no-progress "$rollup_native_package@$rollup_version")
        fi

        runHook postBuild
      '';

      installPhase = ''
        runHook preInstall

        # Capture both the interface node_modules (with @spacedrive/* symlinks
        # pointing sideways) and the spaceui node_modules (transitive deps
        # that those symlinks chain through).
        mkdir -p $out/interface $out/spaceui
        cp -r interface/node_modules $out/interface/node_modules
        cp -r spaceui/node_modules $out/spaceui/node_modules
        if [ -d spaceui/packages/primitives/node_modules ]; then
          mkdir -p $out/spaceui/packages
          for pkg in tokens primitives forms icons ai explorer; do
            if [ -d spaceui/packages/$pkg/node_modules ]; then
              mkdir -p $out/spaceui/packages/$pkg
              cp -r spaceui/packages/$pkg/node_modules $out/spaceui/packages/$pkg/node_modules
            fi
          done
        fi

        runHook postInstall
      '';

      outputHash = hash;
      outputHashAlgo = "sha256";
      outputHashMode = "recursive";
    };

  # Default frontend node modules with fixed hash
  frontendNodeModulesDefault = frontendNodeModules {};

  frontend = stdenv.mkDerivation {
    pname = "spacebot-frontend";
    inherit version;
    src = frontendSrc;

    nativeBuildInputs = with pkgs; [
      bun
      nodejs
    ];

    dontConfigure = true;

    buildPhase = ''
      runHook preBuild

      # Stage node_modules for both workspaces. interface/ symlinks
      # `@spacedrive/*` into spaceui/packages/* (workspace protocol); those
      # symlinks chain through spaceui/packages/*/node_modules for transitive
      # deps like @radix-ui/* and framer-motion.
      cp -r ${frontendNodeModulesDefault}/interface/node_modules interface/
      cp -r ${frontendNodeModulesDefault}/spaceui/node_modules spaceui/
      if [ -d ${frontendNodeModulesDefault}/spaceui/packages ]; then
        for pkg in tokens primitives forms icons ai explorer; do
          if [ -d ${frontendNodeModulesDefault}/spaceui/packages/$pkg/node_modules ]; then
            cp -r ${frontendNodeModulesDefault}/spaceui/packages/$pkg/node_modules spaceui/packages/$pkg/
          fi
        done
      fi
      chmod -R u+w interface/node_modules spaceui

      patchShebangs --build interface/node_modules
      if [ -d spaceui/node_modules ]; then patchShebangs --build spaceui/node_modules; fi

      # spaceui workspace packages (primitives, forms, ai, explorer) export
      # ./dist/index.js via their package.json "exports". Vite's workspace
      # resolve in interface/ reads those exports and needs the built files
      # on disk. Filter to packages/* only: examples (showcase) and the
      # storybook workspace have tsc setups that fail under Nix sandbox
      # and are not required for interface builds.
      (cd spaceui && bunx turbo run build --filter="./packages/*")

      cd interface
      bun run build

      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall

      mkdir -p $out
      cp -r dist/* $out/

      runHook postInstall
    '';
  };

  commonRustBuildEnv = ''
    export ORT_LIB_LOCATION=${onnxruntime}/lib
    export CARGO_PROFILE_RELEASE_LTO=off
    export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=256
  '';

  commonRustBuildEnvWithLinker =
    commonRustBuildEnv
    + lib.optionalString stdenv.isLinux ''
      if [ -n "''${RUSTFLAGS:-}" ]; then
        export RUSTFLAGS="$RUSTFLAGS -C link-arg=-fuse-ld=mold"
      else
        export RUSTFLAGS="-C link-arg=-fuse-ld=mold"
      fi
    '';

  commonBuildEnv = ''
    export SPACEBOT_SKIP_FRONTEND_BUILD=1
    mkdir -p interface/dist
    cp -r ${frontend}/* interface/dist/
  '';

  commonBuildEnvWithLinker = commonRustBuildEnvWithLinker + commonBuildEnv;

  # Post-patch hook that replaces the vendored imap-proto with our patched version
  postPatchImapProto = ''
    replace_imap_proto() {
      local dir="$1"
      echo "Found imap-proto at: $dir"
      if [ -f "$dir/Cargo.toml" ]; then
        echo "Replacing with patched version"
        rm -rf "$dir"
        cp -r ${cargoSrc}/vendor/imap-proto-0.10.2 "$dir"
        chmod -R u+w "$dir"
      fi
    }
    export -f replace_imap_proto

    # Find and replace imap-proto in cargo vendor directories
    # Use specific version to avoid accidental matches and NIX_BUILD_TOP for robustness
    find "''${NIX_BUILD_TOP:-/build}" -type d -name "imap-proto-0.10.2" 2>/dev/null -exec bash -c 'replace_imap_proto "$0"' {} \;

    # Also check cargo home
    if [ -d "$CARGO_HOME/registry/src" ]; then
      find "$CARGO_HOME/registry/src" -type d -name "imap-proto-0.10.2" 2>/dev/null -exec bash -c 'replace_imap_proto "$0"' {} \;
    fi

    # Check in cargo vendor dir if set
    if [ -n "''${cargoVendorDir:-}" ] && [ -d "$cargoVendorDir" ]; then
      find "$cargoVendorDir" -type d -name "imap-proto-0.10.2" 2>/dev/null -exec bash -c 'replace_imap_proto "$0"' {} \;
    fi
  '';

  spacebot = craneLib.buildPackage {
    src = cargoSrc;
    inherit nativeBuildInputs buildInputs;
    strictDeps = true;
    cargoExtraArgs = "";

    doCheck = false;
    cargoBuildCommand = "cargo build --release --bin spacebot";

    postPatch = postPatchImapProto;

    preBuild = commonBuildEnvWithLinker;

    postInstall = ''
      mkdir -p $out/share/spacebot
      cp -r ${runtimeAssetsSrc}/prompts $out/share/spacebot/
      cp -r ${runtimeAssetsSrc}/migrations $out/share/spacebot/
      chmod -R u+w $out/share/spacebot
    '';

    meta = with lib; {
      description = "An AI agent for teams, communities, and multi-user environments";
      homepage = "https://spacebot.sh";
      license = {
        shortName = "FSL-1.1-ALv2";
        fullName = "Functional Source License, Version 1.1, ALv2 Future License";
        url = "https://fsl.software/";
        free = true;
        redistributable = true;
      };
      platforms = platforms.linux ++ platforms.darwin;
      mainProgram = "spacebot";
    };
  };

  spacebot-tests = craneLib.cargoTest {
    src = cargoSrc;
    inherit nativeBuildInputs buildInputs;
    strictDeps = true;

    doCheck = true;

    postPatch = postPatchImapProto;

    # Skip tests that require ONNX model file and known flaky suites in Nix builds
    cargoTestExtraArgs = "-- --skip memory::search::tests --skip memory::store::tests --skip config::tests::test_llm_provider_tables_parse_with_env_and_lowercase_keys";

    preBuild = commonBuildEnvWithLinker;
  };

  spacebot-full = pkgs.symlinkJoin {
    name = "spacebot-full";
    paths = [spacebot];

    buildInputs = [pkgs.makeWrapper];

    postBuild = ''
      wrapProgram $out/bin/spacebot \
        --set CHROME_PATH "${pkgs.chromium}/bin/chromium" \
        --set CHROME_FLAGS "--no-sandbox --disable-dev-shm-usage --disable-gpu" \
        --set ORT_LIB_LOCATION "${onnxruntime}/lib" \
        --prefix LD_LIBRARY_PATH : ${onnxruntime}/lib
    '';

    meta =
      spacebot.meta
      // {
        description = spacebot.meta.description + " (with browser support)";
      };
  };
in {
  inherit frontend frontendNodeModules spacebot spacebot-full spacebot-tests;
}
