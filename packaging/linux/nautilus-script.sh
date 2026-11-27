#!/bin/sh
set -eu

case "$(basename "$0")" in
    "ZAR - Open Archive") action=open ;;
    "ZAR - Extract here") action=extract-here ;;
    "ZAR - Extract to...") action=extract-to ;;
    "ZAR - Create Archive...") action=create ;;
    "ZAR - Convert to ZAR") action=convert-to-zar ;;
    "ZAR - Mount Archive") action=mount ;;
    *) echo "Unknown ZAR Nautilus action." >&2; exit 2 ;;
esac

if [ "$action" = "mount" ]; then
    # Mount expects a single archive; use the first selected file.
    if [ "$#" -eq 0 ] && [ -n "${NAUTILUS_SCRIPT_SELECTED_FILE_PATHS:-}" ]; then
        old_ifs=$IFS
        IFS='
'
        set -f
        set -- $NAUTILUS_SCRIPT_SELECTED_FILE_PATHS
        IFS=$old_ifs
    fi
    if [ "$#" -eq 0 ]; then
        echo "Select a ZAR archive first." >&2
        exit 2
    fi
    exec /usr/bin/zarmount --ask "$1"
fi

if [ "$#" -eq 0 ] && [ -n "${NAUTILUS_SCRIPT_SELECTED_FILE_PATHS:-}" ]; then
    old_ifs=$IFS
    IFS='
'
    set -f
    # Nautilus supplies one local path per line; newline-containing names cannot
    # be represented by this script interface.
    set -- $NAUTILUS_SCRIPT_SELECTED_FILE_PATHS
    IFS=$old_ifs
fi

if [ "$#" -eq 0 ]; then
    echo "Select one or more local files or folders first." >&2
    exit 2
fi

exec /usr/bin/zarg --context "$action" -- "$@"
