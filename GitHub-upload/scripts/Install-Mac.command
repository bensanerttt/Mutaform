#!/bin/bash
# Double-click to install Mutaform for the current user (VST3 + CLAP).
set -e
cd "$(dirname "$0")"
VST3="$HOME/Library/Audio/Plug-Ins/VST3"
CLAP="$HOME/Library/Audio/Plug-Ins/CLAP"
mkdir -p "$VST3" "$CLAP"
rm -rf "$VST3/Mutaform.vst3" "$CLAP/Mutaform.clap"
cp -R Mutaform.vst3 "$VST3/"
cp -R Mutaform.clap "$CLAP/"
# Files downloaded from the internet are quarantined by macOS; this plugin isn't notarized,
# so remove the quarantine flag so DAWs are allowed to load it.
xattr -cr "$VST3/Mutaform.vst3" "$CLAP/Mutaform.clap" 2>/dev/null || true
echo ""
echo "Mutaform installed to:"
echo "  $VST3/Mutaform.vst3"
echo "  $CLAP/Mutaform.clap"
echo "Open FL Studio and run Options > Manage plugins > Find more plugins."
