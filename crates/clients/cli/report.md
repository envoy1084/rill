# rill bug report

- rill 0.1.0
- platform: macos aarch64
- settings: color "auto", pager "default", 0 alias(es)

## What happened

(describe what you did, what you expected and what happened)

## rill doctor

```json
{
  "server": {
    "endpoint": "http://127.0.0.1:54076",
    "reachable": true,
    "principal": "dev",
    "latency_ms": 0.593,
    "error": null
  },
  "daemon": {
    "running": true,
    "pid": 35680,
    "version": "0.1.0",
    "dir": "/var/folders/mq/zg4qxz3j5n5gyqg76sfhn59m0000gn/T/.tmpg5ctRm"
  },
  "cache": {
    "objects": 5,
    "bytes": 32,
    "budget_bytes": 1073741824,
    "free_bytes": 160709701632,
    "evictions": 0
  },
  "tiers": {
    "local": {
      "reads": 0,
      "p50_ms": 0.0,
      "p99_ms": 0.0
    },
    "regional": {
      "reads": 0,
      "p50_ms": 0.0,
      "p99_ms": 0.0
    },
    "origin": {
      "reads": 0,
      "p50_ms": 0.0,
      "p99_ms": 0.0
    },
    "regional_fallbacks": 0
  },
  "checkouts": [
    {
      "path": "/private/var/folders/mq/zg4qxz3j5n5gyqg76sfhn59m0000gn/T/.tmpoK7WUc/everyday",
      "repo": "everyday",
      "view": "eager",
      "base": "9606bac696cb4e1d6db5c6b06eab6128b9b5ca6bb63d21781786afbb131a2e03",
      "pending": 1,
      "last_sync_ms": null,
      "sync_error": null
    }
  ],
  "warnings": []
}
```

## Daemon log (last 0 lines)

```

```
