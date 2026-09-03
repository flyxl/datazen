/** Auto-split domain: backup (es) */
const pack = {
  'backup.startRestore': 'Iniciar restauración…',
  'backup.restoring': 'Restaurando…',
  'backup.restoreSuccess': 'Restauración completada',
  'backup.fileName': 'Nombre del archivo',
  'backup.fileNameHint': 'Haga clic para cambiar el patrón del nombre del archivo.',
  'backup.searchConnection': 'Buscar conexión…',
  'backup.searchDatabase': 'Buscar base de datos…',
  'backup.selectConnectionFirst': 'Seleccione una conexión primero',
  'backup.addOption': 'Agregar opción…',
  'backup.compressGzip': 'Comprimir archivo usando Gzip',
  'backup.startBackup': 'Iniciar copia de seguridad…',
  'backup.inProgress': 'Copia de seguridad…',
  'backup.success': 'Copia de seguridad completada',
  'backup.unsupportedType': 'Este tipo de conexión no soporta copias de seguridad',
  'backup.progressPreparing': 'Preparando copia de seguridad…',
  'backup.progressObject': 'Volcando {name} ({current}/{total})',
  'backup.progressWriting': 'Escribiendo archivo de copia…',
  'backup.restoreOverwriteConfirm':
    'La base de datos destino "{database}" ya tiene {count} objetos. Continuar eliminará las tablas/vistas existentes y aplicará la copia de seguridad. ¿Sobrescribir?',
  'backup.restorePreparing': 'Leyendo archivo de copia de seguridad…',
  'backup.restoreProgress': 'Restaurando {name} ({current}/{total})',
  'backup.progressLog': 'Registro de ejecución',
  'backup.copyLog': 'Copiar registro',
  'backup.logCopied': 'Copied',
  'backup.logOmitted': '… {count} líneas omitidas (registro recortado para ahorrar memoria)',
} as const;
export default pack;
