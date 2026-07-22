#!/usr/bin/env bash
set -euo pipefail

readonly -a CODEX_VOICE_PID_RECORDS=(
  codex-voice.pid
  codex-voice-overlay.pid
  codex-voice-preview-overlay.pid
  codex-voice-transcriber.pid
  codex-voice-typing.pid
  codex-voice-session-owner.pid
)

record_uid() {
  stat -c %u -- "$1" 2>/dev/null
}

process_uid() {
  stat -c %u -- "$1/$2" 2>/dev/null
}

process_stat() {
  local proc_root="$1" pid="$2"
  [[ -r "$proc_root/$pid/stat" ]] || return 1
  IFS= read -r REPLY < "$proc_root/$pid/stat"
  printf '%s\n' "$REPLY"
}

process_start_time() {
  local stat_text="$1" after_name
  local -a fields
  after_name="${stat_text##*) }"
  [[ "$after_name" != "$stat_text" ]] || return 1
  read -r -a fields <<< "$after_name"
  [[ "${fields[19]:-}" =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "${fields[19]}"
}

record_process_name_matches() {
  local record="$1" stat_text="$2" prefix process_name
  prefix="${stat_text% "${stat_text##*) }"}"
  process_name="${prefix#*(}"
  process_name="${process_name%)}"
  case "$record" in
    codex-voice.pid) [[ "$process_name" == arecord ]] ;;
    codex-voice-overlay.pid|codex-voice-preview-overlay.pid)
      [[ "$process_name" == python3 || "$process_name" == python3.* ]]
      ;;
    codex-voice-transcriber.pid) [[ "$process_name" == codex-asr ]] ;;
    codex-voice-typing.pid) [[ "$process_name" == ydotool ]] ;;
    codex-voice-session-owner.pid) [[ "$process_name" == codex-voice ]] ;;
    *) return 1 ;;
  esac
}

legacy_record_is_newer_than_process() {
  local path="$1" start_time="$2" proc_root="$3" clock_ticks="$4"
  local boot_time record_time process_started
  boot_time="$(while read -r key value _; do
    [[ "$key" == btime ]] && { printf '%s\n' "$value"; break; }
  done < "$proc_root/stat")" || return 1
  [[ "$boot_time" =~ ^[0-9]+$ && "$clock_ticks" =~ ^[1-9][0-9]*$ ]] || return 1
  record_time="$(stat -c %Y -- "$path" 2>/dev/null)" || return 1
  [[ "$record_time" =~ ^[0-9]+$ ]] || return 1
  process_started=$((boot_time + start_time / clock_ticks))
  ((record_time >= process_started))
}

active_record_pid() {
  local path="$1" expected_uid="$2" proc_root="$3" clock_ticks="$4"
  local reject_shared_writes="${5:-false}"
  local contents pid recorded_start_time="" actual_start_time stat_text
  local owner process_owner mode size
  local json_pattern='^\{"pid":([1-9][0-9]*),"startTime":([1-9][0-9]*)\}$'

  [[ -f "$path" && ! -L "$path" ]] || return 1
  size="$(stat -c %s -- "$path" 2>/dev/null)" || return 1
  [[ "$size" =~ ^[0-9]+$ ]] || return 1
  ((size <= 512)) || return 1
  if "$reject_shared_writes"; then
    mode="$(stat -c %a -- "$path" 2>/dev/null)" || return 1
    [[ "$mode" =~ ^[0-7]{3,4}$ ]] || return 1
    (( (8#$mode & 8#22) == 0 )) || return 1
  fi

  contents="$(tr -d '[:space:]' < "$path" 2>/dev/null)" || return 1
  if [[ "$contents" =~ $json_pattern ]]; then
    pid="${BASH_REMATCH[1]}"
    recorded_start_time="${BASH_REMATCH[2]}"
  elif [[ "$contents" =~ ^[1-9][0-9]*$ ]]; then
    pid="$contents"
  else
    return 1
  fi
  ((${#pid} <= 10)) || return 1

  owner="$(record_uid "$path")" || return 1
  process_owner="$(process_uid "$proc_root" "$pid")" || return 1
  [[ "$owner" == "$process_owner" ]] || return 1
  [[ -z "$expected_uid" || "$owner" == "$expected_uid" ]] || return 1

  stat_text="$(process_stat "$proc_root" "$pid")" || return 1
  actual_start_time="$(process_start_time "$stat_text")" || return 1
  record_process_name_matches "${path##*/}" "$stat_text" || return 1
  if [[ -n "$recorded_start_time" ]]; then
    [[ "$recorded_start_time" == "$actual_start_time" ]] || return 1
  else
    legacy_record_is_newer_than_process \
      "$path" "$actual_start_time" "$proc_root" "$clock_ticks" || return 1
  fi
  printf '%s\n' "$pid"
}

active_runtime_directory_pid() {
  local runtime_dir="$1" expected_uid="$2" proc_root="$3" clock_ticks="$4"
  local reject_shared_writes="${5:-false}" record pid
  [[ -d "$runtime_dir" ]] || return 1
  for record in "${CODEX_VOICE_PID_RECORDS[@]}"; do
    if pid="$(active_record_pid \
      "$runtime_dir/$record" "$expected_uid" "$proc_root" "$clock_ticks" \
      "$reject_shared_writes")"; then
      printf '%s\n' "$pid"
      return 0
    fi
  done
  return 1
}

account_entries() {
  local passwd_file="${1:-}"
  if [[ -n "$passwd_file" ]]; then
    command cat -- "$passwd_file"
  elif command -v getent >/dev/null 2>&1; then
    getent passwd
  else
    command cat /etc/passwd
  fi
}

active_environment_runtime_pid() {
  local proc_root="$1" clock_ticks="$2" tmp_dir="$3" process_dir uid env_fd entry
  local runtime_set runtime cache home directory pid reject_shared_writes
  for process_dir in "$proc_root"/[0-9]*; do
    [[ -d "$process_dir" ]] || continue
    uid="$(process_uid "$proc_root" "${process_dir##*/}")" || continue
    runtime_set=false
    runtime=""
    cache=""
    home=""
    exec {env_fd}< "$process_dir/environ" 2>/dev/null || continue
    while IFS= read -r -d '' -u "$env_fd" entry; do
      case "$entry" in
        XDG_RUNTIME_DIR=*) runtime_set=true; runtime="${entry#*=}" ;;
        XDG_CACHE_HOME=*) cache="${entry#*=}" ;;
        HOME=*) home="${entry#*=}" ;;
      esac
    done
    exec {env_fd}<&-
    if "$runtime_set"; then
      directory="$runtime"
    elif [[ -n "$cache" ]]; then
      directory="$cache/codex-voice/runtime"
    elif [[ -n "$home" ]]; then
      directory="$home/.cache/codex-voice/runtime"
    else
      continue
    fi
    [[ "$directory" == /* ]] || continue
    reject_shared_writes=false
    [[ "${directory%/}" == "${tmp_dir%/}" ]] && reject_shared_writes=true
    if pid="$(active_runtime_directory_pid \
      "$directory" "$uid" "$proc_root" "$clock_ticks" "$reject_shared_writes")"; then
      printf '%s\n' "$pid"
      return 0
    fi
  done
  return 1
}

active_upgrade_pid() {
  local run_root="$1" tmp_dir="$2" proc_root="$3" passwd_file="${4:-}"
  local runtime_dir uid _ name home shell clock_ticks pid
  clock_ticks="$(getconf CLK_TCK 2>/dev/null || true)"
  [[ "$clock_ticks" =~ ^[1-9][0-9]*$ ]] || clock_ticks=100

  for runtime_dir in "$run_root"/[0-9]*; do
    [[ -d "$runtime_dir" ]] || continue
    uid="${runtime_dir##*/}"
    if pid="$(active_runtime_directory_pid "$runtime_dir" "$uid" "$proc_root" "$clock_ticks")"; then
      printf '%s\n' "$pid"
      return 0
    fi
  done
  while IFS=: read -r name _ uid _ _ home shell; do
    [[ "$uid" =~ ^[0-9]+$ && "$home" == /* ]] || continue
    if pid="$(active_runtime_directory_pid \
      "$home/.cache/codex-voice/runtime" "$uid" "$proc_root" "$clock_ticks")"; then
      printf '%s\n' "$pid"
      return 0
    fi
  done < <(account_entries "$passwd_file")
  if pid="$(active_environment_runtime_pid "$proc_root" "$clock_ticks" "$tmp_dir")"; then
    printf '%s\n' "$pid"
    return 0
  fi
  # Version 0.1.0 used /tmp. Ownership and process-name checks above prevent
  # another user from blocking upgrades with a record pointing at a victim PID.
  active_runtime_directory_pid "$tmp_dir" "" "$proc_root" "$clock_ticks" true
}

main() {
  local action="${1:-install}" active_pid
  case "$action" in
    install|upgrade) ;;
    *) return 0 ;;
  esac

  if [[ "$action" == upgrade ]] && active_pid="$(active_upgrade_pid /run/user /tmp /proc)"; then
    echo "Codex Voice is active (PID $active_pid). Stop or cancel dictation, then retry the upgrade." >&2
    return 1
  fi

  local legacy_target="/opt/Codex Voice Settings/codex-voice-settings"
  local legacy_launcher="/usr/bin/codex-voice-settings"
  if command -v update-alternatives >/dev/null 2>&1; then
    update-alternatives --remove codex-voice-settings "$legacy_target" || true
  elif [[ -L "$legacy_launcher" && "$(readlink -f "$legacy_launcher")" == "$legacy_target" ]]; then
    rm -f "$legacy_launcher"
  fi

  rm -f /usr/share/applications/codex-voice-settings.desktop

  if [[ -f /etc/apparmor.d/codex-voice-settings ]]; then
    if command -v apparmor_parser >/dev/null 2>&1; then
      apparmor_parser --remove /etc/apparmor.d/codex-voice-settings || true
    fi
    rm -f /etc/apparmor.d/codex-voice-settings
  fi
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
