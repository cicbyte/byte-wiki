"""pytest 公共 fixtures

提供：
- bs_bin: bs 可执行文件路径
- bs_env: 隔离的 BYTE_WIKI_HOME 环境变量
- bs_data: session 级共享数据目录（所有测试用同一个库，模拟真实使用流程）
- yaml_data: 加载 YAML 测试数据
"""
import os
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

import pytest
import yaml


# ============ 路径常量 ============

PROJECT_ROOT = Path(__file__).parent.parent
TESTS_DIR = Path(__file__).parent


def _find_bs() -> str:
    """查找 bs 可执行文件"""
    env_bin = os.environ.get("BS_BIN")
    if env_bin and Path(env_bin).exists():
        return env_bin
    # 常见位置
    candidates = [
        PROJECT_ROOT / "target" / "debug" / "bs",
        PROJECT_ROOT / "target" / "debug" / "bs.exe",
        PROJECT_ROOT / "apps" / "cli" / "target" / "debug" / "bs",
    ]
    for c in candidates:
        if c.exists():
            return str(c)
    pytest.exit(f"找不到 bs 可执行文件，请设置 BS_BIN 环境变量或先 cargo build", returncode=1)


# ============ Session 级 fixtures ============

@pytest.fixture(scope="session")
def bs_bin() -> str:
    """bs 可执行文件路径"""
    return _find_bs()


@pytest.fixture(scope="session")
def bs_data(tmp_path_factory) -> Path:
    """session 级隔离数据目录，所有测试共享同一个知识库

    模拟真实使用流程：创建 → 搜索 → 删除 → 恢复 → 备份
    """
    data_dir = tmp_path_factory.mktemp("bs-data")
    return data_dir


@pytest.fixture(scope="session")
def bs_env(bs_data) -> dict:
    """带 BYTE_WIKI_HOME 的环境变量"""
    env = os.environ.copy()
    env["BYTE_WIKI_HOME"] = str(bs_data)
    return env


# ============ YAML 数据加载 ============

@pytest.fixture(scope="session")
def yaml_data() -> dict:
    """加载所有 YAML 测试数据"""
    data = {}
    data_dir = TESTS_DIR / "data"
    for yml in data_dir.glob("*.yaml"):
        with open(yml, encoding="utf-8") as f:
            data[yml.stem] = yaml.safe_load(f)
    return data


# ============ 便捷 fixtures ============

@pytest.fixture(scope="session")
def created_ids() -> dict:
    """session 级存储已创建的实体 ID，供后续测试引用"""
    return {"snippets": [], "notes": [], "categories": [], "projects": []}
