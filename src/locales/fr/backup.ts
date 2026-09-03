/** Auto-split domain: backup (fr) */
const pack = {
  'backup.startRestore': 'Démarrer la restauration…',
  'backup.restoring': 'Restauration…',
  'backup.restoreSuccess': 'Restauration terminée',
  'backup.fileName': 'Nom de fichier',
  'backup.fileNameHint': 'Cliquez pour modifier le modèle de nom de fichier',
  'backup.searchConnection': 'Rechercher une connexion…',
  'backup.searchDatabase': 'Rechercher une base de données…',
  'backup.selectConnectionFirst': "Sélectionnez d'abord une connexion",
  'backup.addOption': 'Ajouter une option…',
  'backup.compressGzip': "Compresser le fichier à l'aide de Gzip",
  'backup.startBackup': 'Démarrer la sauvegarde…',
  'backup.inProgress': 'Sauvegarde…',
  'backup.success': 'Sauvegarde terminée',
  'backup.unsupportedType':
    'Ce type de connexion ne prend pas en charge la sauvegarde de base de données',
  'backup.progressPreparing': 'Préparation de la sauvegarde…',
  'backup.progressObject': 'Dump de {name} ({current}/{total})',
  'backup.progressWriting': 'Écriture du fichier de sauvegarde…',
  'backup.restoreOverwriteConfirm':
    'La base de données cible "{database}" contient déjà {count} objets. Continuer supprimera les tables/vues existantes puis appliquera la sauvegarde. Écraser ?',
  'backup.restorePreparing': 'Lecture du fichier de sauvegarde…',
  'backup.restoreProgress': 'Restauration de {name} ({current}/{total})',
  'backup.progressLog': "Journal d'exécution",
  'backup.copyLog': 'Copier le journal',
  'backup.logCopied': 'Copied',
  'backup.logOmitted': '… {count} lignes omises (journal tronqué pour économiser la mémoire)',
} as const;
export default pack;
