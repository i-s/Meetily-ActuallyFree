import { normaliseLanguageCode } from '@/lib/summary-languages';
import {
  detectTranscriptSummaryLanguage,
  readCachedDetectedSummaryLanguage,
  readMeetingSummaryLanguage,
  readPinnedSummaryLanguageDefault,
} from '@/lib/summary-language-preferences';

/** Saved meetings keep their own Auto/explicit choice; the pin applies to live/new meetings only. */
export async function resolveAskAiLanguage(meetingId: string | undefined, transcriptTexts: string[]): Promise<string> {
  if (meetingId) {
    try {
      const explicit = (await readMeetingSummaryLanguage(meetingId)).language;
      if (explicit) return explicit;
    } catch { /* A missing preference still permits transcript detection. */ }
    try {
      const cached = await readCachedDetectedSummaryLanguage(meetingId);
      if (cached) return cached;
    } catch { /* Continue with the transcript currently available. */ }
  } else {
    const pinned = readPinnedSummaryLanguageDefault();
    if (pinned) return pinned;
  }
  try {
    return (await detectTranscriptSummaryLanguage(transcriptTexts)).language ?? 'en';
  } catch {
    return 'en';
  }
}

const ENGLISH = {
  tab: 'Ask AI', placeholder: 'Ask a question…', emptyTitle: 'Ask anything',
  reading: 'Reading the records…', failed: 'Couldn’t answer:', requestFailed: 'Request failed',
  retry: 'Try again', clear: 'Clear conversation', send: 'Send',
  noTranscript: 'Nothing has been said yet. Ask again once the transcript has some lines.',
  live: {
    emptyTitle: 'Ask about this call',
    emptyHint: 'Answers come from what has been said so far and your notes.',
    suggestions: ['What did I miss in the last 5 minutes?', 'What has been decided?', 'Which action items came up?', 'What questions are still open?'],
    placeholder: 'Ask about the call…',
    footnote: 'Uses your summary model. Cloud providers receive the parts of the transcript needed to answer.',
  },
  meeting: {
    emptyTitle: 'Ask about this meeting',
    emptyHint: 'Answers use the whole transcript, your notes, the summary, and the action items, with links to the moment.',
    suggestions: ['What was decided?', 'Who owns what?', 'What questions are still open?', 'Summarize the last ten minutes'],
    placeholder: 'Ask about this meeting…',
    footnote: 'Uses your configured AI model. Cloud providers receive the parts of this meeting needed to answer.',
  },
};

const RUSSIAN: typeof ENGLISH = {
  tab: 'Спросить ИИ', placeholder: 'Задайте вопрос…', emptyTitle: 'Задайте любой вопрос',
  reading: 'Изучаю материалы…', failed: 'Не удалось ответить:', requestFailed: 'Ошибка запроса',
  retry: 'Повторить', clear: 'Очистить переписку', send: 'Отправить',
  noTranscript: 'В расшифровке пока нет текста. Задайте вопрос, когда появятся первые реплики.',
  live: {
    emptyTitle: 'Спросите об этом разговоре',
    emptyHint: 'Ответы основаны на уже прозвучавших репликах и ваших заметках.',
    suggestions: ['Что я пропустил за последние 5 минут?', 'О чём договорились?', 'Какие задачи появились?', 'Какие вопросы остались открытыми?'],
    placeholder: 'Вопрос о разговоре…',
    footnote: 'Используется модель для итогов встреч. Облачные сервисы получают нужные для ответа части расшифровки.',
  },
  meeting: {
    emptyTitle: 'Спросите об этой встрече',
    emptyHint: 'Ответы основаны на расшифровке, заметках, итогах и задачах, со ссылками на нужный момент.',
    suggestions: ['Что было решено?', 'Кто за что отвечает?', 'Какие вопросы остались открытыми?', 'Подведите итоги последних десяти минут'],
    placeholder: 'Вопрос о встрече…',
    footnote: 'Используется выбранная модель ИИ. Облачные сервисы получают нужные для ответа материалы встречи.',
  },
};

/** Only Ask AI copy is localized; all supported preference codes still drive model answers. */
export function askAiCopy(language: string | null | undefined) {
  return normaliseLanguageCode(language) === 'ru' ? RUSSIAN : ENGLISH;
}
