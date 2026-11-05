#!/bin/sh
# In the Flatpak SDK, Qt lives in /usr but Qt WebEngine comes from the base app
# in /app. cxx-qt asks qmake for one library folder and one header folder, so
# this makes merged ones (symlinks to both) and a qmake that points at them.
#   merged-qmake.sh <dir>   ->   <dir>/qmake
set -eu
dir=$1
real=$(command -v qmake6)
libs=$("$real" -query QT_INSTALL_LIBS)
headers=$("$real" -query QT_INSTALL_HEADERS)

mkdir -p "$dir/lib" "$dir/include"
ln -sf "$libs"/* "$dir/lib/"
ln -sf "$headers"/* "$dir/include/"
for found in /app/lib/libQt6* /app/lib/*-linux-gnu/libQt6*; do
    [ -e "$found" ] && ln -sf "$found" "$dir/lib/"
done
for found in /app/include/Qt*; do
    [ -e "$found" ] && ln -sfn "$found" "$dir/include/"
done

cat > "$dir/qmake" <<EOF
#!/bin/sh
if [ "\${1:-}" = "-query" ]; then
    case "\${2:-}" in
        QT_INSTALL_LIBS*) echo "$dir/lib"; exit 0 ;;
        QT_INSTALL_HEADERS*) echo "$dir/include"; exit 0 ;;
    esac
fi
exec "$real" "\$@"
EOF
chmod +x "$dir/qmake"
