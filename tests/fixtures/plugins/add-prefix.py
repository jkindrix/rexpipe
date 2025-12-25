#!/usr/bin/env python3
# Plugin: add-prefix
# Adds a prefix to each line
import sys

prefix = "[PROCESSED] "
for line in sys.stdin:
    print(prefix + line.rstrip())
