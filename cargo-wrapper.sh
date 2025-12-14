#!/usr/bin/env zsh

autoload -U colors && colors
RED=$(print -P "%F{red}")
GREEN=$(print -P "%F{green}")
YELLOW=$(print -P "%F{yellow}")
BLUE=$(print -P "%F{blue}")
RESET=$(print -P "%f")

if [[ "$LC_CTYPE" == *UTF-8* ]]; then
    EMOJI_INFO="ℹ️"
    EMOJI_WARN="⚠️"
    EMOJI_OK="✅ "
else
    EMOJI_INFO="[i]"
    EMOJI_WARN="[!]"
    EMOJI_OK="[ok]"
fi

if [[ -f "./env.sh" ]]; then
    echo "${BLUE}${EMOJI_INFO} Sourcing env.sh...${RESET}"
    source ./env.sh
fi

NEW_ARGS=()
RUN_BUILD=1
PROFILE_SPECIFIED=0

for arg in "$@"; do
    case "$arg" in
        "--no-r2r"|"--no-ros2")
            echo "${YELLOW}${EMOJI_WARN} Replacing $arg with '--no-default-features --features no-r2r'${RESET}"
            NEW_ARGS+=(--no-default-features --features no-r2r)
            RUN_BUILD=0
            ;;
        "--release"|--profile=*)
            PROFILE_SPECIFIED=1
            NEW_ARGS+=("$arg")
            ;;
        *)
            NEW_ARGS+=("$arg")
            ;;
    esac
done

if [[ $PROFILE_SPECIFIED -eq 0 ]]; then
    NEW_ARGS+=(--release)
fi

if [[ $RUN_BUILD -eq 1 && -f "./build.sh" ]]; then
    echo "${YELLOW}${EMOJI_WARN} Detected r2r/ros2 param and build.sh exists, running build.sh...${RESET}"
    OUTPUT=$(zsh ./build.sh 2>&1)
        STATUS=$?
    if (( STATUS != 0 )); then
        echo "${RED}${EMOJI_WARN} build.sh failed, output:${RESET}"
        echo "$OUTPUT"
        exit 1
    else
        echo "${GREEN}${EMOJI_OK} build.sh finished successfully${RESET}"
    fi
    if zsh ./build.sh &>/dev/null; then
        echo "${GREEN}${EMOJI_OK} build.sh finished successfully${RESET}"
    else
        echo "${RED}${EMOJI_WARN} build.sh failed${RESET}"
        exit 1
    fi
fi

# 安全打印 cargo 命令
echo -n "${BLUE}${EMOJI_INFO} Running: cargo "
for arg in "${NEW_ARGS[@]}"; do
    echo -n "$arg "
done
echo "${RESET}"

cargo "${NEW_ARGS[@]}"
