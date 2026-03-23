"""
proxctl: CLI for Proxmox VE.
"""

try:
    from importlib.metadata import version
    __version__ = version("proxctl")
except ImportError:
    from importlib_metadata import version
    __version__ = version("proxctl")
