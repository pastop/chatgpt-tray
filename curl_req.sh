(
    set +x
    umask 077

    TOKEN=$(jq -r '.tokens.access_token // empty' ~/.codex/auth.json)

    if [ -z "$TOKEN" ]; then
        echo "Не найден access_token"
        exit 1
    fi

    curl -sS \
        -H "Authorization: Bearer $TOKEN" \
        -H "Accept: application/json" \
        -w '\nHTTP_STATUS: %{http_code}\n' \
        https://chatgpt.com/backend-api/wham/usage

    unset TOKEN
)
