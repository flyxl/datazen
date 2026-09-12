const onboarding = {
  // Brand sidebar
  'onboarding.sidebar.heading': 'Bienvenue sur DataZen',
  'onboarding.sidebar.desc':
    'Vous avez déjà un client de base de données ? Importez vos connexions en quelques secondes — ou commencez de zéro. Tout reste sur cette machine.',
  'onboarding.sidebar.importTitle': 'Import en un clic',
  'onboarding.sidebar.importDesc': 'DBeaver, DataGrip, Navicat, TablePlus.',
  'onboarding.sidebar.aiTitle': 'IA native',
  'onboarding.sidebar.aiDesc': 'Assistance SQL sensible au schéma.',
  'onboarding.sidebar.dashboardTitle': 'Tableaux de bord & Graphiques',
  'onboarding.sidebar.dashboardDesc':
    'Transformez vos résultats de requêtes en graphiques, rapports simplifiés.',

  // S0 — Welcome
  'onboarding.s0.title': 'Comment souhaitez-vous commencer ?',
  'onboarding.s0.subtitle':
    "La plupart des utilisateurs importent depuis leur client existant. Les autres méthodes ne prennent qu'une minute.",
  'onboarding.s0.importCard': 'Importer des connexions',
  'onboarding.s0.importCardDesc': 'Depuis DBeaver, DataGrip, Navicat, TablePlus ou un fichier',
  'onboarding.s0.importFastest': 'LE PLUS RAPIDE',
  'onboarding.s0.importDetectedApp': '✓ Configuration {app} trouvée sur cette machine',
  'onboarding.s0.manualCard': 'Créer une connexion manuellement',
  'onboarding.s0.manualCardDesc': 'PostgreSQL, MySQL, SQLite, Redis et plus',
  'onboarding.s0.sampleCard': 'Explorer avec des données exemples',
  'onboarding.s0.sampleCardDesc': 'Un terrain de jeu SQLite local — aucune configuration',

  // S1 — Connect
  'onboarding.s1.title': 'Créez votre première connexion',
  'onboarding.s1.subtitle':
    "Le même formulaire que Nouvelle connexion — les mêmes validations, les mêmes tests. <b>Vous pouvez continuer après l'enregistrement.</b>",
  'onboarding.s1.stepLabel': 'Étape 1 sur 2',
  'onboarding.s1.driver': 'Pilote',
  'onboarding.s1.testBtn': 'Tester la connexion',
  'onboarding.s1.testSuccess': 'Connecté',
  'onboarding.s1.testFail': 'Échec de la connexion',
  'onboarding.s1.importTitle': 'Importez vos connexions',
  'onboarding.s1.importSubtitle':
    'Sélectionnez le client source — ou un fichier de connexion. Les connexions atterrissent dans votre espace de travail, rien ne quitte cette machine.',
  'onboarding.s1.importFileSource': 'Fichier',
  'onboarding.s1.importing': 'Import en cours…',
  'onboarding.s1.importDetecting': 'Recherche de la configuration client…',
  'onboarding.s1.importSuccess': '✓ {count} connexions importées · {source}',
  'onboarding.s1.importUpdated': '✓ {count} connexions mises à jour · {source}',
  'onboarding.s1.sampleTitle': 'Votre terrain de jeu exemple',
  'onboarding.s1.sampleSubtitle':
    "Une base SQLite avec une table demo_sales en anglais — pas de serveur, pas d'identifiants, prêt à interroger.",
  'onboarding.s1.samplePreparing': 'Préparation des données exemples…',
  'onboarding.s1.sampleReady': 'Données exemples prêtes',
  'onboarding.s1.sampleReadyDesc':
    'demo_sales(region, amount, quarter) · 4 régions × 2 trimestres — connecté et en attente dans votre espace de travail.',
  'onboarding.s1.sampleConnection': 'Connexion : {name} · SQLite',
  'onboarding.s1.sampleFailed': 'Impossible de préparer les données exemples',

  // S2 — AI
  'onboarding.s2.title': "Configurez l'assistance IA",
  'onboarding.s2.subtitle':
    "DataZen utilise votre propre clé API pour générer du SQL, expliquer les erreurs et analyser les plans de requête. Ollama fonctionne localement et n'a pas besoin de clé. Vous pouvez ignorer cette étape et la configurer plus tard.",
  'onboarding.s2.stepLabel': 'Étape 2 sur 2',
  'onboarding.s2.optionalHint':
    "Optionnel — vous pouvez configurer l'IA plus tard dans Paramètres → IA.",
  'onboarding.s2.securityNote':
    "Votre clé est chiffrée avec AES-256-GCM et stockée localement. Les requêtes vont directement à votre fournisseur — jamais via les serveurs DataZen. Il n'en existe pas.",

  // S3 — Done
  'onboarding.s3.title': 'Tout est prêt',
  'onboarding.s3.subtitle':
    'Votre espace de travail est prêt. DataZen ouvrira votre connexion et vous pourrez commencer à écrire des requêtes immédiatement.',
  'onboarding.s3.connLabel': 'Connexion',
  'onboarding.s3.aiLabel': 'Fournisseur IA',
  'onboarding.s3.storageLabel': 'Stockage',
  'onboarding.s3.storageValue': 'Chiffré localement · AES-256-GCM',
  'onboarding.s3.importedValue': '{count} connexions importées · {source}',
  'onboarding.s3.updatedValue': '{count} connexions mises à jour · {source}',
  'onboarding.s3.aiNotConfigured': 'Non configuré (optionnel)',
  'onboarding.s3.notConfigured': 'Non configuré (peut être ajouté plus tard)',
  'onboarding.s3.openBtn': 'Ouvrir DataZen',
  'onboarding.s3.nextHint':
    'Étape suivante : la requête est prête — appuyez sur <b>Exécuter</b> puis <b>Ajouter au tableau de bord</b>',

  // Common
  'onboarding.common.skip': 'Ignorer la configuration',
  'onboarding.common.back': 'Retour',
  'onboarding.common.continue': 'Continuer',
  'onboarding.common.finish': 'Terminer',
} as const;

export default onboarding;
