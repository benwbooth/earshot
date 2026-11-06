{
  description = "Earshot: a modern desktop music player (Rust + Qt Quick via cxx-qt)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        qtModules = with pkgs.qt6; [
          qtbase
          qtdeclarative
          qtmultimedia
          qtsvg
          qtwebengine
          qtwebchannel
          qtpositioning
        ] ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.qt6.qtwayland pkgs.kdePackages.layer-shell-qt ];
        qtEnv = pkgs.symlinkJoin { name = "earshot-qt-env"; paths = qtModules; };
        # cxx-qt asks qmake where Qt lives; answer with the merged module tree
        # so QtQml/QtQuick headers and tools from every module are found.
        qmake = pkgs.writeShellScriptBin "earshot-qmake6" ''
          if [ "$1" = "-query" ]; then
            case "$2" in
              QT_HOST_LIBEXECS*|QT_INSTALL_LIBEXECS*) echo "${qtEnv}/libexec"; exit 0 ;;
              QT_HOST_BINS*|QT_INSTALL_BINS*) echo "${qtEnv}/bin"; exit 0 ;;
              QT_INSTALL_LIBS*) echo "${qtEnv}/lib"; exit 0 ;;
              QT_INSTALL_PREFIX*) echo "${qtEnv}"; exit 0 ;;
              QT_INSTALL_QML*) echo "${qtEnv}/lib/qt-6/qml"; exit 0 ;;
              QT_INSTALL_HEADERS*) echo "${qtEnv}/include"; exit 0 ;;
            esac
          fi
          exec ${qtEnv}/bin/qmake6 "$@"
        '';
        src = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./build.rs ./src ./crates ./qml ./native ./packaging ./scripts ];
        };

        # The web version's interface: Rust compiled to WebAssembly, with its
        # page, styles, the Qt app's icons and the Inter font.
        web = pkgs.rustPlatform.buildRustPackage {
          pname = "earshot-web";
          version = "0.1.0";
          inherit src;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.wasm-bindgen-cli pkgs.lld pkgs.woff2 ];
          buildPhase = ''
            runHook preBuild
            CARGO_OFFLINE=1 INTER_TTF=${pkgs.inter}/share/fonts/truetype/InterVariable.ttf \
              bash scripts/build-web.sh $out --release
            runHook postBuild
          '';
          installPhase = "runHook preInstall; runHook postInstall";
          doCheck = false;
        };

        # The web version's server, which also serves the interface.
        server = pkgs.rustPlatform.buildRustPackage {
          pname = "earshot-server";
          version = "0.1.0";
          inherit src;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "earshot-server" ];
          doCheck = false;
          postInstall = ''
            mkdir -p $out/share/earshot
            ln -s ${web} $out/share/earshot/web
          '';
          meta = {
            description = "Earshot's web version: the music player in a browser, on desktops and phones";
            license = lib.licenses.mit;
            mainProgram = "earshot-server";
          };
        };
              in
      {
        packages.web = web;
        packages.server = server;

        # The server as a container image, for ~/src/home-services:
        #   nix build .#docker && docker load < result
        # The library lives in /data; it listens on port 8790.
        packages.docker = pkgs.dockerTools.buildLayeredImage {
          name = "earshot";
          tag = "latest";
          contents = [ pkgs.cacert ];
          extraCommands = "mkdir -p data tmp && chmod 1777 tmp";
          config = {
            Cmd = [ "${server}/bin/earshot-server" ];
            Env = [
              "EARSHOT_LISTEN=0.0.0.0:8790"
              "EARSHOT_DATA_DIR=/data"
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            ];
            ExposedPorts."8790/tcp" = { };
            Volumes."/data" = { };
          };
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "earshot";
          version = "0.1.0";
          inherit src;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ qmake pkgs.pkg-config pkgs.qt6.wrapQtAppsHook pkgs.rustPlatform.bindgenHook ];
          buildInputs = qtModules ++ [ pkgs.libglvnd pkgs.pipewire pkgs.alsa-lib ];

          QMAKE = "${qmake}/bin/earshot-qmake6";
          QT_VERSION_MAJOR = "6";
          dontWrapQtApps = true;
          # Qt's setup hooks point QMAKE at qtbase alone; use the merged tree.
          preBuild = ''
            export QMAKE=${qmake}/bin/earshot-qmake6
          '';
          preCheck = ''
            export QMAKE=${qmake}/bin/earshot-qmake6
          '';

          postInstall = ''
            install -Dm644 packaging/earshot.desktop $out/share/applications/earshot.desktop
            install -Dm644 packaging/earshot.svg $out/share/icons/hicolor/scalable/apps/earshot.svg
            # The web version, for "Open the web version" in the settings menu.
            mkdir -p $out/share/earshot
            ln -s ${web} $out/share/earshot/web
          '';

          postFixup = ''
            wrapQtApp $out/bin/earshot
          '';

          meta = {
            description = "A modern desktop music player with charts, Reddit radio, albums and playlists";
            license = lib.licenses.mit;
            mainProgram = "earshot";
          };
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/earshot";
        };

        # The web version on this computer: nix run .#server, then open http://127.0.0.1:8790/
        apps.server = {
          type = "app";
          program = "${server}/bin/earshot-server";
        };

        devShells.default = pkgs.mkShell {
          packages = [ pkgs.cargo pkgs.rustc pkgs.clippy pkgs.rustfmt pkgs.wasm-bindgen-cli pkgs.lld pkgs.woff2 pkgs.pkg-config pkgs.libglvnd pkgs.pipewire pkgs.alsa-lib pkgs.clang pkgs.mold qmake qtEnv ];
          hardeningDisable = [ "fortify" ];
          shellHook = ''
            export QMAKE=${qmake}/bin/earshot-qmake6
            export QT_VERSION_MAJOR=6
            unset QML2_IMPORT_PATH
            export QT_FORCE_STDERR_LOGGING=1
            export INTER_TTF=${pkgs.inter}/share/fonts/truetype/InterVariable.ttf
            export LIBCLANG_PATH=${pkgs.llvmPackages.libclang.lib}/lib
            export QT_PLUGIN_PATH=${qtEnv}/${pkgs.qt6.qtbase.qtPluginPrefix}
            export QML_IMPORT_PATH=${qtEnv}/${pkgs.qt6.qtbase.qtQmlPrefix}
          '';
        };
      });
}
