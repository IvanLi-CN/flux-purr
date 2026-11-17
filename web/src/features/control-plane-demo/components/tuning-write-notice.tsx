import { Square } from 'lucide-react'
import './tuning-write-notice.css'

export interface TuningWriteNoticeProps {
  stopping: boolean
  error?: string
  onStop: () => void
}

/** Ordinary status drives this notice without consuming the tuning event journal. */
export function TuningWriteNotice({ stopping, error, onStop }: TuningWriteNoticeProps) {
  return (
    <section className="tuning-write-notice" aria-label="调优运行保护">
      <output>
        <strong>固件热控调优正在运行</strong>
        <span>其他写操作已停用，仍可查看状态或停止调优。</span>
        {error ? <span role="alert">{error}</span> : null}
      </output>
      <button
        type="button"
        className="industrial-button industrial-button--secondary"
        disabled={stopping}
        onClick={onStop}
      >
        <Square size={14} aria-hidden="true" />
        {stopping ? '正在停止…' : '停止调优'}
      </button>
    </section>
  )
}
