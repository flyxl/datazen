const onboarding = {
  // Brand sidebar
  'onboarding.sidebar.heading': 'Willkommen bei DataZen',
  'onboarding.sidebar.desc':
    'Haben Sie bereits einen Datenbank-Client? Importieren Sie Ihre Verbindungen in Sekunden — oder starten Sie neu. Alles bleibt auf diesem Gerät.',
  'onboarding.sidebar.importTitle': 'Ein-Klick-Import',
  'onboarding.sidebar.importDesc': 'DBeaver, DataGrip, Navicat, TablePlus.',
  'onboarding.sidebar.aiTitle': 'KI-nativ',
  'onboarding.sidebar.aiDesc': 'Schema-awaree SQL-Assistenz.',
  'onboarding.sidebar.dashboardTitle': 'Dashboards & Diagramme',
  'onboarding.sidebar.dashboardDesc':
    'Abfrageergebnisse sofort in Diagramme umwandeln und Berichte einfach teilen.',

  // S0 — Welcome
  'onboarding.s0.title': 'Wie möchten Sie starten?',
  'onboarding.s0.subtitle':
    'Die meisten Benutzer importieren vom bereits verwendeten Client. Der Rest dauert nur eine Minute.',
  'onboarding.s0.importCard': 'Verbindungen importieren',
  'onboarding.s0.importCardDesc': 'Von DBeaver, DataGrip, Navicat, TablePlus oder einer Datei',
  'onboarding.s0.importFastest': 'SCHNELLSTES',
  'onboarding.s0.importDetectedApp': '✓ {app}-Konfiguration auf diesem Gerät gefunden',
  'onboarding.s0.manualCard': 'Verbindung manuell erstellen',
  'onboarding.s0.manualCardDesc': 'PostgreSQL, MySQL, SQLite, Redis und mehr',
  'onboarding.s0.sampleCard': 'Mit Beispieldaten ausprobieren',
  'onboarding.s0.sampleCardDesc': 'Ein lokales SQLite-Spielplatz — keine Konfiguration nötig',

  // S1 — Connect
  'onboarding.s1.title': 'Erste Verbindung erstellen',
  'onboarding.s1.subtitle':
    'Gleiche Formular wie bei Neue Verbindung — gleiche Validierung, gleicher Test. <b>Nach dem Speichern können Sie fortfahren.</b>',
  'onboarding.s1.stepLabel': 'Schritt 1 von 2',
  'onboarding.s1.driver': 'Treiber',
  'onboarding.s1.testBtn': 'Verbindung testen',
  'onboarding.s1.testSuccess': 'Verbunden',
  'onboarding.s1.testFail': 'Verbindung fehlgeschlagen',
  'onboarding.s1.importTitle': 'Ihre Verbindungen importieren',
  'onboarding.s1.importSubtitle':
    'Wählen Sie den Client, von dem Sie migrieren — oder eine Verbindungsdatei. Die Verbindungen landen in Ihrem Arbeitsbereich, nichts verlässt dieses Gerät.',
  'onboarding.s1.importFileSource': 'Datei',
  'onboarding.s1.importing': 'Importiere…',
  'onboarding.s1.importDetecting': 'Suche nach Client-Konfiguration…',
  'onboarding.s1.importSuccess': '✓ {count} Verbindungen importiert · {source}',
  'onboarding.s1.importUpdated': '✓ {count} Verbindungen aktualisiert · {source}',
  'onboarding.s1.sampleTitle': 'Ihr Beispiel-Spielplatz',
  'onboarding.s1.sampleSubtitle':
    'Eine gebündelte SQLite-Datenbank mit einer englischen demo_sales-Tabelle — kein Server, keine Anmeldeinformationen, sofort abfragebereit.',
  'onboarding.s1.samplePreparing': 'Beispieldaten werden vorbereitet…',
  'onboarding.s1.sampleReady': 'Beispieldaten sind bereit',
  'onboarding.s1.sampleReadyDesc':
    'demo_sales(region, amount, quarter) · 4 Regionen × 2 Quartale — verbunden und in Ihrem Arbeitsbereich bereit.',
  'onboarding.s1.sampleConnection': 'Verbindung: {name} · SQLite',
  'onboarding.s1.sampleFailed': 'Beispieldaten konnten nicht vorbereitet werden',

  // S2 — AI
  'onboarding.s2.title': 'KI-Assistenz einrichten',
  'onboarding.s2.subtitle':
    'DataZen verwendet Ihren eigenen API-Schlüssel zur Generierung von SQL, Fehlererklärung und Analyse von Abfrageplänen. Ollama läuft lokal und benötigt keinen Schlüssel. Sie können dies überspringen und später konfigurieren.',
  'onboarding.s2.stepLabel': 'Schritt 2 von 2',
  'onboarding.s2.optionalHint':
    'Optional — Sie können KI später in Einstellungen → KI konfigurieren.',
  'onboarding.s2.securityNote':
    'Ihr Schlüssel wird mit AES-256-GCM verschlüsselt und lokal gespeichert. Anfragen gehen direkt an Ihren Anbieter — nie über DataZen-Server. Denn es gibt keine.',

  // S3 — Done
  'onboarding.s3.title': 'Alles bereit',
  'onboarding.s3.subtitle':
    'Ihr Arbeitsbereich ist startklar. DataZen öffnet Ihre Verbindung und Sie können sofort mit Abfragen beginnen.',
  'onboarding.s3.connLabel': 'Verbindung',
  'onboarding.s3.aiLabel': 'KI-Anbieter',
  'onboarding.s3.storageLabel': 'Speicher',
  'onboarding.s3.storageValue': 'Lokal verschlüsselt · AES-256-GCM',
  'onboarding.s3.importedValue': '{count} Verbindungen importiert · {source}',
  'onboarding.s3.updatedValue': '{count} Verbindungen aktualisiert · {source}',
  'onboarding.s3.aiNotConfigured': 'Nicht konfiguriert (optional)',
  'onboarding.s3.notConfigured': 'Nicht konfiguriert (kann später hinzugefügt werden)',
  'onboarding.s3.openBtn': 'DataZen öffnen',
  'onboarding.s3.nextHint':
    'Nächster Schritt: Die Abfrage ist bereit — drücken Sie <b>Ausführen</b> und dann <b>Zum Dashboard hinzufügen</b>',

  // Common
  'onboarding.common.skip': 'Einrichtung überspringen',
  'onboarding.common.back': 'Zurück',
  'onboarding.common.continue': 'Weiter',
  'onboarding.common.finish': 'Fertig',
} as const;

export default onboarding;
