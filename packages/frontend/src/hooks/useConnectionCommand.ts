import { useEffect, useState } from 'react';
import { driverCommands } from '../commands/driver';
import type { DriverCommandDefinition } from '../types';

export function useConnectionCommand(dbSessionId: string | undefined, commandId: string) {
  const [definition, setDefinition] = useState<DriverCommandDefinition | undefined>();
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!dbSessionId || !commandId) {
      setDefinition(undefined);
      return;
    }

    let cancelled = false;
    setLoading(true);
    void driverCommands
      .getConnectionCommands(dbSessionId)
      .then((definitions) => definitions.find((item) => item.id === commandId))
      .then((found) => {
        if (!cancelled) setDefinition(found);
      })
      .catch(() => {
        if (!cancelled) setDefinition(undefined);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [dbSessionId, commandId]);

  return { definition, loading };
}

export function useConnectionCommands(dbSessionId: string | undefined) {
  const [definitions, setDefinitions] = useState<DriverCommandDefinition[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!dbSessionId) {
      setDefinitions([]);
      return;
    }

    let cancelled = false;
    setLoading(true);
    void driverCommands
      .getConnectionCommands(dbSessionId)
      .then((items) => {
        if (!cancelled) setDefinitions(items);
      })
      .catch(() => {
        if (!cancelled) setDefinitions([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [dbSessionId]);

  return { definitions, loading };
}
