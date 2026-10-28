import { useEffect, useState, useRef } from 'react';
import { motion } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { StatusCard } from './StatusCard';
import { QuickActions } from './QuickActions';
import { SystemInfo } from './SystemInfo';
import { Setup } from '../Setup';
import { api, ServiceStatus, isTauri } from '../../lib/tauri';
import { Terminal, RefreshCw, ChevronDown, ChevronUp } from 'lucide-react';
import clsx from 'clsx';
import { EnvironmentStatus } from '../../App';
import { serviceLogger } from '../../lib/logger';

interface DashboardProps {
  envStatus: EnvironmentStatus | null;
  onSetupComplete: () => void;
}

export function Dashboard({ envStatus, onSetupComplete }: DashboardProps) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<ServiceStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [logsExpanded, setLogsExpanded] = useState(true);
  const [autoRefreshLogs, setAutoRefreshLogs] = useState(true);
  const logsContainerRef = useRef<HTMLDivElement>(null);

  const fetchStatus = async () => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    try {
      const result = await api.getServiceStatus();
      setStatus(result);
    } catch (error) {
      serviceLogger.error('读取服务状态失败', error);
    } finally {
      setLoading(false);
    }
  };

  const fetchLogs = async () => {
    if (!isTauri()) return;
    try {
      const result = await invoke<string[]>('get_logs', { lines: 50 });
      setLogs(result);
    } catch (error) {
      serviceLogger.error('读取运行日志失败', error);
    }
  };

  useEffect(() => {
    fetchStatus();
    fetchLogs();
    if (!isTauri()) return;

    const statusInterval = setInterval(fetchStatus, 3000);
    const logsInterval = autoRefreshLogs || actionLoading ? setInterval(fetchLogs, 1000) : null;

    return () => {
      clearInterval(statusInterval);
      if (logsInterval) clearInterval(logsInterval);
    };
  }, [autoRefreshLogs, actionLoading]);

  // 自动滚动到日志底部（仅在日志容器内部滚动，不影响页面）
  useEffect(() => {
    if (logsExpanded && logsContainerRef.current) {
      logsContainerRef.current.scrollTop = logsContainerRef.current.scrollHeight;
    }
  }, [logs, logsExpanded]);

  const handleStart = async () => {
    if (!isTauri()) return;
    setActionError(null);
    setLogsExpanded(true);
    setActionLoading(true);
    try {
      await api.startService();
      await fetchStatus();
      await fetchLogs();
    } catch (e) {
      setActionError(String(e));
      serviceLogger.error('启动失败', e);
    } finally {
      await fetchLogs();
      setActionLoading(false);
    }
  };

  const handleStop = async () => {
    if (!isTauri()) return;
    setActionError(null);
    setActionLoading(true);
    try {
      await api.stopService();
      await fetchStatus();
      await fetchLogs();
    } catch (e) {
      setActionError(String(e));
      serviceLogger.error('停止失败', e);
    } finally {
      setActionLoading(false);
    }
  };

  const handleRestart = async () => {
    if (!isTauri()) return;
    setActionError(null);
    setLogsExpanded(true);
    setActionLoading(true);
    try {
      await api.restartService();
      await fetchStatus();
      await fetchLogs();
    } catch (e) {
      setActionError(String(e));
      serviceLogger.error('重启失败', e);
    } finally {
      await fetchLogs();
      setActionLoading(false);
    }
  };

  const getLogLineClass = (line: string) => {
    if (line.includes('error') || line.includes('Error') || line.includes('ERROR')) {
      return 'text-red-400';
    }
    if (line.includes('warn') || line.includes('Warn') || line.includes('WARN')) {
      return 'text-yellow-400';
    }
    if (line.includes('info') || line.includes('Info') || line.includes('INFO')) {
      return 'text-green-400';
    }
    return 'text-content-secondary';
  };

  const containerVariants = {
    hidden: { opacity: 0 },
    show: {
      opacity: 1,
      transition: {
        staggerChildren: 0.1,
      },
    },
  };

  const itemVariants = {
    hidden: { opacity: 0, y: 20 },
    show: { opacity: 1, y: 0 },
  };

  const needsSetup = envStatus && !envStatus.ready;

  return (
    <div className="h-full overflow-y-auto scroll-container pr-2">
      <motion.div
        variants={containerVariants}
        initial="hidden"
        animate="show"
        className="space-y-6"
      >
        {needsSetup && (
          <motion.div variants={itemVariants}>
            <Setup onComplete={onSetupComplete} embedded />
          </motion.div>
        )}

        <motion.div variants={itemVariants}>
          <StatusCard status={status} loading={loading} />
        </motion.div>

        <motion.div variants={itemVariants}>
          <QuickActions
            status={status}
            loading={actionLoading}
            onStart={handleStart}
            onStop={handleStop}
            onRestart={handleRestart}
          />
          {actionLoading && (
            <p role="status" className="mt-3 text-sm text-content-secondary">正在执行服务操作...</p>
          )}
          {actionError && (
            <pre role="alert" className="mt-3 max-h-64 overflow-auto whitespace-pre-wrap break-all text-sm text-red-400">
              {actionError}
            </pre>
          )}
        </motion.div>

        <motion.div variants={itemVariants}>
          <div className="bg-surface-card rounded-2xl border border-edge overflow-hidden">
            {/* 日志标题栏 */}
            <div 
              className="flex items-center justify-between px-4 py-3 bg-surface-elevated/50 cursor-pointer"
              onClick={() => setLogsExpanded(!logsExpanded)}
            >
              <div className="flex items-center gap-2">
                <Terminal size={16} className="text-content-tertiary" />
                <span className="text-sm font-medium text-content-primary">实时日志</span>
                <span className="text-xs text-content-tertiary">
                  ({logs.length} 行)
                </span>
              </div>
              <div className="flex items-center gap-3">
                {logsExpanded && (
                  <>
                    <label 
                      className="flex items-center gap-2 text-xs text-content-secondary"
                      onClick={e => e.stopPropagation()}
                    >
                      <input
                        type="checkbox"
                        checked={autoRefreshLogs}
                        onChange={(e) => setAutoRefreshLogs(e.target.checked)}
                        className="w-3 h-3 rounded border-edge bg-surface-elevated text-claw-500"
                      />
                      {t('dashboard.autoRefresh')}
                    </label>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        fetchLogs();
                      }}
                      className="text-content-tertiary hover:text-content-primary"
                      title="刷新日志"
                    >
                      <RefreshCw size={14} />
                    </button>
                  </>
                )}
                {logsExpanded ? (
                  <ChevronUp size={16} className="text-content-tertiary" />
                ) : (
                  <ChevronDown size={16} className="text-content-tertiary" />
                )}
              </div>
            </div>

            {logsExpanded && (
              <div ref={logsContainerRef} className="h-64 overflow-y-auto p-4 font-mono text-xs leading-relaxed bg-surface-sidebar">
                {logs.length === 0 ? (
                  <div className="h-full flex items-center justify-center text-content-tertiary">
                    <p>暂无日志，请先启动服务</p>
                  </div>
                ) : (
                  <>
                    {logs.map((line, index) => (
                      <div
                        key={index}
                        className={clsx('py-0.5 whitespace-pre-wrap break-all', getLogLineClass(line))}
                      >
                        {line}
                      </div>
                    ))}

                  </>
                )}
              </div>
            )}
          </div>
        </motion.div>

        <motion.div variants={itemVariants}>
          <SystemInfo />
        </motion.div>
      </motion.div>
    </div>
  );
}
