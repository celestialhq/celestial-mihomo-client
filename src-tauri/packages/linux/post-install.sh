#!/bin/bash
chmod +x /usr/bin/celestial-service-install
chmod +x /usr/bin/celestial-service-uninstall
chmod +x /usr/bin/celestial-service

. /etc/os-release

if [ "$ID" = "deepin" ]; then
    PACKAGE_NAME="$DPKG_MAINTSCRIPT_PACKAGE"
    DESKTOP_FILES=$(dpkg -L "$PACKAGE_NAME" 2>/dev/null | grep "\.desktop$")
    echo "$DESKTOP_FILES" | while IFS= read -r f; do
        if [ "$(basename "$f")" == "Celestial.desktop" ]; then
            echo "Fixing deepin desktop file"
            mv -vf "$f" "/usr/share/applications/celestial.desktop"
        fi
    done
fi
