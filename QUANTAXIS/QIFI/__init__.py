from importlib import import_module

_EXPORTS = {
    'QIFI_Account': ('QUANTAXIS.QIFI.QifiAccount', 'QIFI_Account'),
    'QA_QIFIMANAGER': ('QUANTAXIS.QIFI.QifiManager', 'QA_QIFIMANAGER'),
    'QA_QIFISMANAGER': ('QUANTAXIS.QIFI.QifiManager', 'QA_QIFISMANAGER'),
}


def __getattr__(name):
    if name not in _EXPORTS:
        raise AttributeError(f"module '{__name__}' has no attribute '{name}'")
    module_name, attr_name = _EXPORTS[name]
    value = getattr(import_module(module_name), attr_name)
    globals()[name] = value
    return value


__all__ = ['QIFI_Account', 'QA_QIFIMANAGER', 'QA_QIFISMANAGER']
