"""bs CLI 调用封装

提供 run_bs() 函数，封装 subprocess 调用 + JSON 解析 + 错误断言。
"""
import json
import subprocess


def run_bs(bs_bin: str, args: list[str], json_output: bool = True, env: dict | None = None) -> dict | list | str:
    """调用 bs CLI

    Args:
        bs_bin: bs 可执行文件路径
        args: 子命令参数列表，如 ["snippet", "add", "--title", "test", ...]
        json_output: 是否加 --json 标志（默认 True）
        env: 环境变量（含 BYTE_WIKI_HOME）

    Returns:
        JSON 解析结果（dict/list）或原始 stdout 字符串

    Raises:
        AssertionError: 退出码非 0
        json.JSONDecodeError: JSON 解析失败
    """
    cmd = [bs_bin]
    if json_output:
        cmd.append("--json")
    cmd.extend(args)

    result = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=30)

    assert result.returncode == 0, (
        f"bs {' '.join(args)} 失败（exit {result.returncode}）\n"
        f"stdout: {result.stdout}\nstderr: {result.stderr}"
    )

    if json_output:
        stdout = result.stdout.strip()
        if not stdout:
            return {}
        return json.loads(stdout)
    return result.stdout.strip()
