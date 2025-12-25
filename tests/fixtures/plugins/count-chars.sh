#!/bin/bash
# Plugin: count-chars
# Outputs line with character count appended
while IFS= read -r line; do
    count=${#line}
    echo "$line [$count chars]"
done
