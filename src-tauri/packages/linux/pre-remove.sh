#!/bin/bash
/usr/bin/clash-verge-service-uninstall

. /etc/os-release

if [ "$ID" = "deepin" ]; then
    if [ -f "/usr/share/applications/celestial.desktop" ]; then
        echo "Removing deepin desktop file"
        rm -vf "/usr/share/applications/celestial.desktop"
    fi
fi
