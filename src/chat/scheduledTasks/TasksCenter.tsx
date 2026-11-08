import type { ReactNode } from 'react'
import { MessageSquarePlus, Plus } from 'lucide-react'
import { Button } from '../../components/Button'
import { useT } from '../../components/i18n'
import { setHash } from '../chatRoutes'
import { ScheduledTasksCenter } from './ScheduledTasksCenter'
import type { RegisterTaskLeaveGuard } from './ScheduledTaskEditor'
import { useScheduledTasks } from './useScheduledTasks'
import '../market/market.css'

export function TasksCenter({ tab, onOpenConversation, onCreateByChat, registerLeaveGuard }: {
  tab: 'schedules' | 'automations'
  onOpenConversation: (id: string) => void
  onCreateByChat: (prompt: string) => void
  registerLeaveGuard?: RegisterTaskLeaveGuard
}) {
  const t = useT()
  const { tasks } = useScheduledTasks()
  const active = tab === 'automations' ? 'schedules' : tab

  const shell = (body: ReactNode, actions: ReactNode) => <section className="kv assistant-center-root flex h-full min-h-0 flex-col">
    <header className="shrink-0 px-6 pb-5 pt-5">
      <div className="mx-auto w-full max-w-[832px]">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-3" data-tauri-drag-region="false">
              <h1 className="m-0 text-[26px] font-bold text-[var(--text)]">{t.chatNavTasks}</h1>
              <div className="kv-plugin-segments" role="tablist" aria-label={t.chatNavTasks}>
                <button id={`tasks-tab-${active}`} type="button" role="tab"
                  className="kv-plugin-segment" aria-current="page"
                  aria-selected aria-controls="tasks-panel"
                  tabIndex={0} onClick={() => setHash(`#chat/${active}`)}>
                  <span className="flex items-center gap-2">
                    {t.chatNavSchedules}
                    <span className="text-[11px] opacity-60">· {tasks?.length ?? '—'}</span>
                  </span>
                </button>
              </div>
            </div>
            <p className="mt-1 text-[13px] text-neutral-500 dark:text-neutral-400">{t.chatTasksSubtitle}</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        </div>
        <p className="mt-3 text-[12px] leading-relaxed text-neutral-500 dark:text-neutral-400">{t.chatTasksSchedulesHint}</p>
      </div>
    </header>
    <div id="tasks-panel" role="tabpanel" aria-labelledby={`tasks-tab-${active}`} className="flex min-h-0 flex-1 flex-col">{body}</div>
  </section>

  const createByChatPrompt = () => onCreateByChat(t.chatSchedulesChatPrompt)
  const createByChat = <Button size="sm" variant="primary" onClick={createByChatPrompt}>
    <MessageSquarePlus size={14} strokeWidth={1.75} />{t.chatTasksCreateByChat}
  </Button>

  return <ScheduledTasksCenter onOpenConversation={onOpenConversation} onCreateByChat={createByChatPrompt} registerLeaveGuard={registerLeaveGuard}
    renderList={(body, onCreate) => shell(body, <>
      <Button size="sm" variant="ghost" onClick={onCreate}><Plus size={14} strokeWidth={1.75} />{t.chatTasksCreateManually}</Button>
      {createByChat}
    </>)} />
}
