#!/usr/bin/env python3
"""Read-only HIL probe for exhaustion of a device's HTTP socket pool."""

import argparse
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--device-id", required=True)
    parser.add_argument("--requests", type=int, default=20)
    args = parser.parse_args()
    url = urllib.parse.urlsplit(args.base_url)
    if (
        url.scheme != "http"
        or not url.hostname
        or url.username is not None
        or url.password is not None
        or url.query
        or url.fragment
        or url.path not in ("", "/")
        or not 1 <= args.requests <= 100
    ):
        parser.error("provide an HTTP device origin without credentials and 1..100 requests")
    endpoint = args.base_url.rstrip("/") + "/health"
    # Probes never follow a redirect to a different device or send credentials.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            return None

    opener = urllib.request.build_opener(NoRedirect())
    failures = []
    passed = 0
    for index in range(args.requests):
        try:
            with opener.open(endpoint, timeout=3) as response:
                health = json.load(response)
                if health.get("deviceId") != args.device_id:
                    print(json.dumps({"error": "device_identity_mismatch", "request": index + 1}))
                    return 1
                if response.status != 200 or health.get("ok") is not True:
                    raise ValueError("health response is not ready")
            passed += 1
        except (OSError, ValueError, urllib.error.URLError) as error:
            failures.append({"request": index + 1, "error": str(error)})
        time.sleep(0.05)
    print(json.dumps({"passed": passed, "failed": len(failures), "errors": failures}))
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(main())
