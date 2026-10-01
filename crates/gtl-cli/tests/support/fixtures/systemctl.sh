#!/bin/sh
action=$2
if [ "$action" = show-environment ]; then
    env
    exit 0
fi
case "$action" in
    show)
        if [ "${5:-}" = --property=Environment ]; then
            printf 'ExecStart={ path=%s ; argv[]=%s ; }\nEnvironment=\nEnvironmentFiles=\nWorkingDirectory=\n' "$GTL_TEST_SERVER" "$GTL_TEST_SERVER"
        elif [ "${4:-}" = --property=UnitFileState ]; then
            if [ -f "$GTL_TEST_STATE" ] && [ "${GTL_TEST_ENABLED:-1}" = 1 ]; then echo enabled; else echo disabled; fi
        elif [ ! -f "$GTL_TEST_STATE" ]; then
            echo LoadState=not-found
        else
            echo LoadState=loaded
            state=$(cat "$GTL_TEST_STATE")
            if [ "$state" = running ]; then echo ActiveState=active; echo MainPID=1; else echo ActiveState=inactive; echo MainPID=0; fi
        fi
        ;;
    enable) echo stopped > "$GTL_TEST_STATE" ;;
    start) echo running > "$GTL_TEST_STATE" ;;
    stop) echo stopped > "$GTL_TEST_STATE" ;;
    disable) rm -f "$GTL_TEST_STATE" ;;
    daemon-reload) : ;;
    *) exit 1 ;;
esac
