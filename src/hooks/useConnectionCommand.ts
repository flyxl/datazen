import { useEffect, useState } from 'react';
import { driverCommands } from '../commands/driver';
import type { DriverCommandDefinition } from '../types';

export function useConnectionCommand(connectionId: string | undefined, commandId: string) {
  const [definition, setDefinition] = useState<DriverCommandDefinition | undefined>();
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!connectionId || !commandId) {
      setDefinition(undefined);
      return;
    }

    let cancelled = false;
    setLoading(true);
    void driverCommands
      .getConnectionCommands(connectionId)
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
  }, [connectionId, commandId]);

  return { definition, loading };
}

export function useConnectionCommands(connectionId: string | undefined) {
  const [definitions, setDefinitions] = useState<DriverCommandDefinition[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!connectionId) {
      setDefinitions([]);
      return;
    }

    let cancelled = false;
    setLoading(true);
    void driverCommands
      .getConnectionCommands(connectionId)
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
  }, [connectionId]);

  return { definitions, loading };
}
