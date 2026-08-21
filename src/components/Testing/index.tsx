import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { motion } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import {
  CheckCircle,
  XCircle,
  Play,
  Loader2,
  Stethoscope,
  Wrench,
} from 'lucide-react';
import clsx from 'clsx';
import { testingLogger } from '../../lib/logger';
import { api, DoctorFixResult } from '../../lib/tauri';

interface DiagnosticResult {
  name: string;
  passed: boolean;
  message: string;
  suggestion: string | null;
}

export function Testing() {
  const { t } = useTranslation();
  const [diagnosticResults, setDiagnosticResults] = useState<DiagnosticResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [fixing, setFixing] = useState(false);
  const [fixResult, setFixResult] = useState<DoctorFixResult | null>(null);

  const runDiagnostics = async () => {
    testingLogger.action('运行系统诊断');
    testingLogger.info('开始系统诊断...');
    setLoading(true);
    setDiagnosticResults([]);
    try {
      const results = await invoke<DiagnosticResult[]>('run_doctor');
      testingLogger.info(`诊断完成，共 ${results.length} 项检查`);
      const passed = results.filter(r => r.passed).length;
      testingLogger.state('诊断结果', { total: results.length, passed, failed: results.length - passed });
      setDiagnosticResults(results);
    } catch (e) {
      testingLogger.error('诊断执行失败', e);
      setDiagnosticResults([{
        name: t('testing.diagnosticExecution'),
        passed: false,
        message: String(e),
        suggestion: t('testing.checkInstallation'),
      }]);
    } finally {
      setLoading(false);
    }
  };

  const fixConfiguration = async () => {
    if (!window.confirm('将先备份 openclaw.json，再执行 OpenClaw 自动修复并重启网关。是否继续？')) return;

    setFixing(true);
    setFixResult(null);
    try {
      const result = await api.fixOpenclawConfig();
      setFixResult(result);
      await runDiagnostics();
    } catch (error) {
      setFixResult({
        success: false,
        message: `自动修复失败: ${String(error)}`,
        backup_path: null,
        restart_message: null,
      });
    } finally {
      setFixing(false);
    }
  };

  // 统计结果
  const passedCount = diagnosticResults.filter(r => r.passed).length;
  const failedCount = diagnosticResults.filter(r => !r.passed).length;

  return (
    <div className="h-full overflow-y-auto scroll-container pr-2">
      <div className="max-w-4xl space-y-6">
        {/* 诊断测试 */}
        <div className="bg-surface-card rounded-2xl p-6 border border-edge">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-xl bg-purple-500/20 flex items-center justify-center">
                <Stethoscope size={20} className="text-purple-400" />
              </div>
              <div>
                <h3 className="text-lg font-semibold text-content-primary">系统诊断</h3>
                <p className="text-xs text-content-tertiary">
                  检查 OpenClaw 安装和配置状态
                </p>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={fixConfiguration}
                disabled={loading || fixing}
                className="btn-secondary flex items-center gap-2"
                title="备份配置后执行 openclaw doctor --fix，并重启网关"
              >
                {fixing ? <Loader2 size={16} className="animate-spin" /> : <Wrench size={16} />}
                自动修复并重启
              </button>
              <button
                onClick={runDiagnostics}
                disabled={loading || fixing}
                className="btn-primary flex items-center gap-2"
              >
                {loading ? <Loader2 size={16} className="animate-spin" /> : <Play size={16} />}
                {t('testing.runDiagnostics')}
              </button>
            </div>
          </div>

          {fixResult && (
            <div className={clsx('mb-4 p-3 rounded-lg text-sm', fixResult.success ? 'bg-green-500/10 text-green-400' : 'bg-red-500/10 text-red-400')}>
              <p>{fixResult.message}</p>
              {fixResult.backup_path && <p className="mt-1 text-xs text-content-tertiary">配置备份：{fixResult.backup_path}</p>}
            </div>
          )}

          {/* 诊断结果统计 */}
          {diagnosticResults.length > 0 && (
            <div className="flex gap-4 mb-4 p-3 bg-surface-elevated rounded-lg">
              <div className="flex items-center gap-2">
                <CheckCircle size={16} className="text-green-400" />
                <span className="text-sm text-green-400">{t('testing.passed', { count: passedCount })}</span>
              </div>
              {failedCount > 0 && (
                <div className="flex items-center gap-2">
                  <XCircle size={16} className="text-red-400" />
                  <span className="text-sm text-red-400">{t('testing.failed', { count: failedCount })}</span>
                </div>
              )}
            </div>
          )}

          {/* 诊断结果列表 */}
          {diagnosticResults.length > 0 && (
            <motion.div
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              className="space-y-2"
            >
              {diagnosticResults.map((result, index) => (
                <div
                  key={index}
                  className={clsx(
                    'flex items-start gap-3 p-3 rounded-lg',
                    result.passed ? 'bg-green-500/10' : 'bg-red-500/10'
                  )}
                >
                  {result.passed ? (
                    <CheckCircle size={18} className="text-green-400 mt-0.5 flex-shrink-0" />
                  ) : (
                    <XCircle size={18} className="text-red-400 mt-0.5 flex-shrink-0" />
                  )}
                  <div className="flex-1 min-w-0">
                    <p
                      className={clsx(
                        'text-sm font-medium',
                        result.passed ? 'text-green-400' : 'text-red-400'
                      )}
                    >
                      {result.name}
                    </p>
                    <p className="text-xs text-content-secondary mt-1 whitespace-pre-wrap break-words">{result.message}</p>
                    {result.suggestion && (
                      <p className="text-xs text-amber-400 mt-1">
                        💡 {result.suggestion}
                      </p>
                    )}
                  </div>
                </div>
              ))}
            </motion.div>
          )}

          {/* 空状态 */}
          {diagnosticResults.length === 0 && !loading && (
            <div className="text-center py-8 text-content-tertiary">
              <Stethoscope size={48} className="mx-auto mb-3 opacity-30" />
              <p>{t('testing.emptyState')}</p>
            </div>
          )}
        </div>

        {/* 说明 */}
        <div className="bg-surface-card/50 rounded-xl p-4 border border-edge">
          <h4 className="text-sm font-medium text-content-secondary mb-2">诊断说明</h4>
          <ul className="text-sm text-content-tertiary space-y-1">
            <li>• 系统诊断会检查 Node.js、OpenClaw 安装、配置文件等状态</li>
            <li>• AI 连接测试请前往 <span className="text-claw-400">AI 配置</span> 页面进行</li>
            <li>• 渠道测试请前往 <span className="text-claw-400">消息渠道</span> 页面进行</li>
          </ul>
        </div>
      </div>
    </div>
  );
}
