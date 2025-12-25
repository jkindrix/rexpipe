#!/bin/bash
# Plugin: reverse-words
# Reverses the order of words in each line
while IFS= read -r line; do
    echo "$line" | awk '{for(i=NF;i>=1;i--) printf "%s ", $i; print ""}'
done
