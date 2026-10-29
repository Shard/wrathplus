"""The doc-link gate must not report green when rustdoc itself failed."""
import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('doc_links', Path(__file__).with_name('doc-links.py'))
doc_links = importlib.util.module_from_spec(spec)
spec.loader.exec_module(doc_links)


class RustdocTests(unittest.TestCase):
    def test_failed_rustdoc_is_not_an_empty_success(self):
        result = subprocess.CompletedProcess([], 101, '', 'compiler error')
        with patch.object(doc_links.subprocess, 'run', return_value=result):
            with self.assertRaisesRegex(RuntimeError, 'compiler error'):
                doc_links.run_rustdoc()

    def test_success_preserves_diagnostics_for_link_checks(self):
        result = subprocess.CompletedProcess([], 0, 'docs', 'warning')
        with patch.object(doc_links.subprocess, 'run', return_value=result):
            self.assertEqual(doc_links.run_rustdoc(), 'warningdocs')


if __name__ == '__main__':
    unittest.main()
