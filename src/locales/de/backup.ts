/** Auto-split domain: backup (de) */
const pack = {
  'backup.startRestore': 'Wiederherstellung starten…',
  'backup.restoring': 'Wird wiederhergestellt…',
  'backup.restoreSuccess': 'Wiederherstellung abgeschlossen',
  'backup.fileName': 'Dateiname',
  'backup.fileNameHint': 'Klicken Sie, um das Dateinamenmuster zu ändern',
  'backup.searchConnection': 'Verbindung suchen…',
  'backup.searchDatabase': 'Nach Datenbank suchen…',
  'backup.selectConnectionFirst': 'Wählen Sie zunächst eine Verbindung aus',
  'backup.addOption': 'Option hinzufügen…',
  'backup.compressGzip': 'Komprimieren Sie die Datei mit Gzip',
  'backup.startBackup': 'Sicherung starten…',
  'backup.inProgress': 'Sichern…',
  'backup.success': 'Sicherung abgeschlossen',
  'backup.unsupportedType': 'Dieser Verbindungstyp unterstützt keine Datenbanksicherung',
  'backup.progressPreparing': 'Sicherung wird vorbereitet…',
  'backup.progressObject': 'Dump von {name} ({current}/{total})',
  'backup.progressWriting': 'Sicherungsdatei wird geschrieben…',
  'backup.restoreOverwriteConfirm':
    'Zieldatenbank "{database}" hat bereits {count} Objekte. Fortfahren löscht bestehende Tabellen/Views und wendet dann die Sicherung an. Überschreiben?',
  'backup.restorePreparing': 'Sicherungsdatei wird gelesen…',
  'backup.restoreProgress': '{name} wird wiederhergestellt ({current}/{total})',
  'backup.progressLog': 'Ausführungsprotokoll',
  'backup.copyLog': 'Protokoll kopieren',
  'backup.logCopied': 'Copied',
  'backup.logOmitted': '… {count} Zeilen ausgelassen (Log zur Speicherschonung gekürzt)',
} as const;
export default pack;
