import { useEffect, useRef, useState } from 'react';
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
  Terminal,
  Shield,
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
  const [runtimeLogs, setRuntimeLogs] = useState<string[]>([]);
  const logsRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!loading && !fixing) return;
    let disposed = false;
    let pending = false;
    const fetchLogs = async () => {
      if (pending) return;
      pending = true;
      try {
        const lines = await invoke<string[]>('get_logs', { lines: 100 });
        if (!disposed) setRuntimeLogs(lines);
      } catch (error) {
        testingLogger.error('读取运行日志失败', error);
      } finally {
        pending = false;
      }
    };
    fetchLogs();
    const timer = setInterval(fetchLogs, 1000);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, [loading, fixing]);

  useEffect(() => {
    if (logsRef.current) logsRef.current.scrollTop = logsRef.current.scrollHeight;
  }, [runtimeLogs]);

  const fetchFinalLogs = async () => {
    try {
      setRuntimeLogs(await invoke<string[]>('get_logs', { lines: 100 }));
    } catch (error) {
      testingLogger.error('读取运行日志失败', error);
    }
  };

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
      await fetchFinalLogs();
      setLoading(false);
    }
  };

  const fixConfiguration = async (elevated = false) => {
    if (!window.confirm(elevated
      ? '将备份配置并请求 Windows 管理员权限，执行 OpenClaw 修复。请在 UAC 提示中确认，并在终端中确认所需操作。是否继续？'
      : '将备份 openclaw.json 并打开修复终端。OpenClaw 修复可能迁移数据库，请在终端中确认所需操作。是否继续？')) return;

    setFixing(true);
    setFixResult(null);
    try {
      const result = await api.fixOpenclawConfig(elevated);
      setFixResult(result);
      await runDiagnostics();
    } catch (error) {
      setFixResult({
        success: false,
        message: `修复终端操作失败: ${String(error)}`,
        backup_path: null,
        restart_message: null,
      });
    } finally {
      await fetchFinalLogs();
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
          <div className="flex items-center justify-between gap-3 flex-wrap mb-4">
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
            <div className="flex items-center gap-2 flex-wrap">
              <button
                onClick={() => fixConfiguration()}
                disabled={loading || fixing}
                className="btn-secondary flex items-center gap-2"
                title="打开可输入的终端执行 openclaw doctor --fix"
              >
                {fixing ? <Loader2 size={16} className="animate-spin" /> : <Wrench size={16} />}
                {fixing ? '修复进行中' : '打开修复终端'}
              </button>
              <button
                onClick={() => fixConfiguration(true)}
                disabled={loading || fixing}
                className="btn-secondary flex items-center gap-2"
                title="请求管理员权限修复计划任务及符号链接权限问题"
              >
                <Shield size={16} />
                管理员修复
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
            <div role="status" className={clsx('mb-4 p-3 rounded-lg text-sm', fixResult.success ? 'bg-green-500/10 text-green-400' : 'bg-red-500/10 text-red-400')}>
              <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-all font-sans">{fixResult.message}</pre>
              {fixResult.backup_path && <p className="mt-1 text-xs text-content-tertiary">配置备份：{fixResult.backup_path}</p>}
            </div>
          )}

          {(loading || fixing || runtimeLogs.length > 0) && (
            <section className="mb-4 border-t border-edge pt-3">
              <div className="flex items-center gap-2 mb-2 text-sm text-content-secondary">
                <Terminal size={16} />
                <span>{fixing ? '自动修复日志' : loading ? '诊断日志' : '运行日志'}</span>
                {(loading || fixing) && <Loader2 size={14} className="animate-spin" />}
              </div>
              <div ref={logsRef} className="h-64 overflow-auto bg-surface-sidebar p-3 font-mono text-xs">
                <pre className="whitespace-pre-wrap break-all">{runtimeLogs.join('\n')}</pre>
              </div>
            </section>
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
