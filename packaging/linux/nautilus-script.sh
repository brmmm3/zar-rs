#!/bin/sh
set -eu

case "$(basename "$0")" in
    "ZAR - Create Archive...") action=create ;;
    "ZAR - Add to Archive...") action=add ;;
    "ZAR - Create Archive (named)") action=create-named ;;
    "ZAR - Open Archive") action=open ;;
    "ZAR - Extract here") action=extract-here ;;
    "ZAR - Extract to...") action=extract-to ;;
    "ZAR - Extract to archive folder") action=extract-to-folder ;;
    "ZAR - Test Archive") action=test ;;
    *) echo "Unknown ZAR Nautilus action." >&2; exit 2 ;;
esac

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
