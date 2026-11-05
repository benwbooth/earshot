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
        ] ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.qt6.qtwayland ];
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
              in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "earshot";
          version = "0.1.0";
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./build.rs ./src ./qml ./native ./packaging ];
          };
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ qmake pkgs.pkg-config pkgs.qt6.wrapQtAppsHook ];
          buildInputs = qtModules ++ [ pkgs.libglvnd ];

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

        devShells.default = pkgs.mkShell {
          packages = [ pkgs.cargo pkgs.rustc pkgs.clippy pkgs.rustfmt pkgs.pkg-config pkgs.libglvnd pkgs.clang pkgs.mold qmake qtEnv ];
          hardeningDisable = [ "fortify" ];
          shellHook = ''
            export QMAKE=${qmake}/bin/earshot-qmake6
            export QT_VERSION_MAJOR=6
            unset QML2_IMPORT_PATH
            export QT_FORCE_STDERR_LOGGING=1
            export QT_PLUGIN_PATH=${qtEnv}/${pkgs.qt6.qtbase.qtPluginPrefix}
            export QML_IMPORT_PATH=${qtEnv}/${pkgs.qt6.qtbase.qtQmlPrefix}
          '';
        };
      });
}
