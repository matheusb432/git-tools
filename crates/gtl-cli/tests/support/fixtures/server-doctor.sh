#!/bin/sh
cat "$GTL_TEST_REPORT"
exit "${GTL_TEST_REPORT_EXIT:-0}"
