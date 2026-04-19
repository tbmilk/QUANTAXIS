# coding: utf-8
"""QUANTAXIS package entrypoint.

重构说明:
1. 顶层 `import QUANTAXIS` 不再立即导入整棵依赖树。
2. 公开 API 仍由 `QUANTAXIS._api` 提供，但只在首次访问相关属性时加载。
3. 这样可以避免因为可选/重量级依赖缺失而在包导入阶段直接失败。
"""

from importlib import import_module
from types import ModuleType

__version__ = '2.1.0.alpha2'
__author__ = 'yutiansut'


def _detect_optional_dependency(module_name):
    try:
        module = import_module(module_name)
    except ImportError:
        return False, None
    return True, getattr(module, '__version__', 'unknown')


__has_qars__, __qars_version__ = _detect_optional_dependency('qars3')
__has_dataswap__, __dataswap_version__ = _detect_optional_dependency('qadataswap')

_LAZY_API_MODULE: ModuleType | None = None


def _load_api() -> ModuleType:
    global _LAZY_API_MODULE
    if _LAZY_API_MODULE is None:
        _LAZY_API_MODULE = import_module('QUANTAXIS._api')
    return _LAZY_API_MODULE


def __getattr__(name):
    api_module = _load_api()
    try:
        value = getattr(api_module, name)
    except AttributeError as exc:
        raise AttributeError(f"module 'QUANTAXIS' has no attribute '{name}'") from exc
    globals()[name] = value
    return value


def __dir__():
    base = set(globals().keys())
    try:
        base.update(dir(_load_api()))
    except Exception:
        pass
    return sorted(base)


def __repr__():
    return f"QUANTAXIS(version='{__version__}', lazy_api=True)"


__str__ = __repr__
