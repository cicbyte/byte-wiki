"""test_08_backup — 备份 / 恢复

验证导出 zip（含 db/bytewiki.db）→ 恢复 → db 可用。
"""
import zipfile
from pathlib import Path

import pytest


@pytest.mark.backup
class TestBackup:
    def test_backup_export(self, bs_bin, bs_env, tmp_path):
        """导出备份 zip，且包含 db/bytewiki.db"""
        from helpers.runner import run_bs

        output = tmp_path / "backup.zip"
        run_bs(bs_bin, ["backup", "--output", str(output)], env=bs_env, json_output=False)
        assert output.exists()
        assert output.stat().st_size > 0
        # zip 里应该有 db 文件
        with zipfile.ZipFile(output) as zf:
            names = zf.namelist()
            assert any("db/bytewiki.db" in n for n in names)

    def test_backup_and_restore(self, bs_bin, bs_env, tmp_path):
        """备份 → 恢复往返，db 仍然可打开（再次备份成功）"""
        from helpers.runner import run_bs

        output = tmp_path / "backup2.zip"
        run_bs(bs_bin, ["backup", "--output", str(output)], env=bs_env, json_output=False)
        assert output.exists()

        run_bs(bs_bin, ["restore", str(output)], env=bs_env, json_output=False)

        again = tmp_path / "backup3.zip"
        run_bs(bs_bin, ["backup", "--output", str(again)], env=bs_env, json_output=False)
        assert again.exists()
