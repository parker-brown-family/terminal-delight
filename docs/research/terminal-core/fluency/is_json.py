"""Exit 0 if the file parses as JSON, 1 otherwise: python3 is_json.py <file>"""
import json
import sys

try:
    json.load(open(sys.argv[1]))
except Exception:
    sys.exit(1)
