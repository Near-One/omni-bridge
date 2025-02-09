#!/usr/bin/env bash

makefile="$1"
shift
selected_pipelines="$@"

if [ $(echo "$selected_pipelines" | wc -w) -eq 1 ]; then
  exit 0
fi

for pipeline in $selected_pipelines; do
  make -f "$makefile" -nd "$pipeline" 2> /dev/null | \
    grep -E 'Must remake target|File .* was considered already' | \
    sed -E \
      -e "s/.*Must remake target '([^']+)'.*/\1/" \
      -e "s/.*File '([^']+)' was considered already.*/\1/" | uniq
done | sort | uniq -c | awk -v count="$(echo "$selected_pipelines" | wc -w)" '$1 == count {print $2}' | paste -sd " "
