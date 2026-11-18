#!/bin/sh
set -eu
case "$(rustc --version)" in 'rustc 1.98.1 '*) ;; *) printf '%s\n' 'Streamline requires rustc 1.98.1 on PATH' >&2; exit 1;; esac
case "$(cargo --version)" in 'cargo 1.98.1 '*) ;; *) printf '%s\n' 'Streamline requires cargo 1.98.1 on PATH' >&2; exit 1;; esac
