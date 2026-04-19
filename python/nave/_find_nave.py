from __future__ import annotations

import os
import sys
import sysconfig


class NaveNotFound(FileNotFoundError): ...


def find_nave_bin() -> str:
    """Return the nave binary path."""

    nave_exe = "nave" + sysconfig.get_config_var("EXE")

    targets = [
        sysconfig.get_path("scripts"),
        sysconfig.get_path("scripts", vars={"base": sys.base_prefix}),
        (
            _join(
                _matching_parents(_module_path(), "Lib/site-packages/nave"),
                "Scripts",
            )
            if sys.platform == "win32"
            else _join(
                _matching_parents(_module_path(), "lib/python*/site-packages/nave"),
                "bin",
            )
        ),
        _join(_matching_parents(_module_path(), "nave"), "bin"),
        sysconfig.get_path("scripts", scheme=_user_scheme()),
    ]

    seen: list[str] = []
    for target in targets:
        if not target:
            continue
        if target in seen:
            continue
        seen.append(target)
        path = os.path.join(target, nave_exe)
        if os.path.isfile(path):
            return path

    locations = "\n".join(f" - {target}" for target in seen)
    raise NaveNotFound(
        f"Could not find the nave binary in any of the following locations:\n{locations}\n"
    )


def _module_path() -> str | None:
    return os.path.dirname(__file__)


def _matching_parents(path: str | None, match: str) -> str | None:
    from fnmatch import fnmatch

    if not path:
        return None
    parts = path.split(os.sep)
    match_parts = match.split("/")
    if len(parts) < len(match_parts):
        return None

    if not all(
        fnmatch(part, match_part)
        for part, match_part in zip(reversed(parts), reversed(match_parts))
    ):
        return None

    return os.sep.join(parts[: -len(match_parts)])


def _join(path: str | None, *parts: str) -> str | None:
    if not path:
        return None
    return os.path.join(path, *parts)


def _user_scheme() -> str:
    if sys.version_info >= (3, 10):
        return sysconfig.get_preferred_scheme("user")
    if os.name == "nt":
        return "nt_user"
    if sys.platform == "darwin" and sys._framework:  # type: ignore[attr-defined]
        return "osx_framework_user"
    return "posix_user"
