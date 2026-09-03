/** Auto-split domain: backup (ru) */
const pack = {
  'backup.startRestore': 'Начать восстановление…',
  'backup.restoring': 'Восстановление…',
  'backup.restoreSuccess': 'Восстановление завершено',
  'backup.fileName': 'Имя файла',
  'backup.fileNameHint': 'Нажмите, чтобы изменить шаблон имени файла',
  'backup.searchConnection': 'Искать соединение…',
  'backup.searchDatabase': 'Искать базу данных…',
  'backup.selectConnectionFirst': 'Сначала выберите соединение',
  'backup.addOption': 'Добавить вариант…',
  'backup.compressGzip': 'Сжать файл с помощью Gzip',
  'backup.startBackup': 'Начать резервное копирование…',
  'backup.inProgress': 'Резервное копирование…',
  'backup.success': 'Резервное копирование завершено',
  'backup.unsupportedType': 'Этот тип подключения не поддерживает резервное копирование',
  'backup.progressPreparing': 'Подготовка резервной копии…',
  'backup.progressObject': 'Дамп {name} ({current}/{total})',
  'backup.progressWriting': 'Запись файла резервной копии…',
  'backup.restoreOverwriteConfirm':
    'Целевая база данных "{database}" уже содержит {count} объектов. При продолжении существующие таблицы/представления будут удалены, а затем применена резервная копия. Перезаписать?',
  'backup.restorePreparing': 'Чтение файла резервной копии…',
  'backup.restoreProgress': 'Восстановление {name} ({current}/{total})',
  'backup.progressLog': 'Журнал выполнения',
  'backup.copyLog': 'Копировать журнал',
  'backup.logCopied': 'Copied',
  'backup.logOmitted': '… {count} строк пропущено (журнал сокращён для экономии памяти)',
} as const;
export default pack;
