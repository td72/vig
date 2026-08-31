#!/usr/bin/env python3
"""mdBook preprocessor: rewrite repo-relative GIF paths for the built site.

The checked-in chapters reference demo GIFs relative to the repository tree
(e.g. `../../../assets/demo.gif`) so they render on github.com's blob view.
The built book instead serves copies under `assets/` next to the pages (put
there by the build step), so this preprocessor rewrites `(../)+assets/` to
`assets/` in every chapter. See the comment in book.toml.
"""

import json
import re
import sys

PATTERN = re.compile(r"(?:\.\./)+assets/")


def rewrite(section):
    chapter = section.get("Chapter")
    if chapter is None:
        return
    chapter["content"] = PATTERN.sub("assets/", chapter["content"])
    for sub in chapter.get("sub_items", []):
        rewrite(sub)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "supports":
        # Supports every renderer.
        sys.exit(0)

    _context, book = json.load(sys.stdin)
    # mdBook <= 0.4 calls the chapter list "sections"; 0.5 calls it "items".
    for section in book.get("items", book.get("sections", [])):
        rewrite(section)
    json.dump(book, sys.stdout)


if __name__ == "__main__":
    main()
