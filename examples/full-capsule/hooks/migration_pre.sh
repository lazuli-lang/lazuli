#!/usr/bin/env bash
# Migrations bucket cycle Route C — pre-migration hook stub.
# The runtime invokes this before applying migrations. Customize per
# deployment; defaults to a no-op so a green-field app passes doctor.
set -euo pipefail
echo "lazuli: pre-migration hook executed at $(date -Iseconds)"
