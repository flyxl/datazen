import { Input } from '../ui/Input';
import { Select } from '../ui/Select';
import { Button } from '../ui/Button';
import { Label } from './shared';
import type { ConnectionFormState } from './useConnectionForm';

export function KiwiConnectionFields({ form }: { form: ConnectionFormState }) {
  return (
    <>
      <div className="md:col-span-2">
        <Label required>Kiwi URL</Label>
        <Input
          value={form.host}
          onChange={(e) => form.setHost(e.target.value)}
          placeholder="https://kiwi.akusre.com"
        />
      </div>
      <div>
        <Label required>Username</Label>
        <Input
          value={form.username}
          onChange={(e) => form.setUsername(e.target.value)}
          placeholder="用户名"
        />
      </div>
      <div>
        <Label required>Password</Label>
        <Input
          type="password"
          value={form.password}
          onChange={(e) => form.setPassword(e.target.value)}
          placeholder="密码"
        />
      </div>
      <div className="md:col-span-2">
        <Button
          variant="secondary"
          onClick={() => void form.handleKiwiLogin()}
          disabled={form.kiwiLoggingIn || !form.host || !form.username || !form.password}
          className="w-full"
        >
          {form.kiwiLoggingIn ? '登录中...' : '登录并加载实例'}
        </Button>
      </div>
      {form.kiwiToken && (
        <div className="md:col-span-2">
          <Label required>Instance Domain</Label>
          {form.kiwiInstances.length > 0 ? (
            <Select
              value={form.database}
              options={form.kiwiInstances.map((inst) => ({
                value: inst.name,
                label: inst.short || inst.alias || inst.name,
              }))}
              onChange={form.setDatabase}
            />
          ) : (
            <div className="flex gap-2">
              <Input
                value={form.database}
                onChange={(e) => form.setDatabase(e.target.value)}
                placeholder="pe-xxx.rwlb.ap-southeast-5.rds.aliyuncs.com"
                className="flex-1"
              />
              <Button
                variant="secondary"
                onClick={() => void form.loadKiwiInstances(form.host, form.kiwiToken)}
                disabled={form.loadingInstances || !form.kiwiToken}
                className="shrink-0 whitespace-nowrap"
              >
                {form.loadingInstances ? '加载中...' : '加载实例'}
              </Button>
            </div>
          )}
        </div>
      )}
    </>
  );
}
