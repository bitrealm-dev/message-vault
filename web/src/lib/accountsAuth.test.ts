import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";

import {
  authenticateAccount,
  accountHasNoPassword,
  createAccount,
  isUsernameTaken,
  setAccountPassword,
} from "./accounts";

describe("account password auth", () => {
  const previousDb = process.env.VAULT_DB;
  const previousDataDir = process.env.VAULT_DATA_DIR;
  let tempDir = "";

  before(() => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "vault-auth-"));
    process.env.VAULT_DB = path.join(tempDir, "vault.db");
    process.env.VAULT_DATA_DIR = path.join(tempDir, "data");
  });

  after(() => {
    if (previousDb === undefined) delete process.env.VAULT_DB;
    else process.env.VAULT_DB = previousDb;
    if (previousDataDir === undefined) delete process.env.VAULT_DATA_DIR;
    else process.env.VAULT_DATA_DIR = previousDataDir;
    fs.rmSync(tempDir, { recursive: true, force: true });
  });

  it("creates password and no-password accounts and authenticates", async () => {
    const withPass = await createAccount({
      username: `pw_${Date.now()}`,
      primaryEmail: `pw_${Date.now()}@example.com`,
      firstName: "Pat",
      lastName: "Word",
      phone: "+15555550111",
      password: "s3cret!",
      noPassword: false,
    });

    const noPass = await createAccount({
      username: `np_${Date.now()}`,
      primaryEmail: `np_${Date.now()}@example.com`,
      firstName: "No",
      lastName: "Pass",
      phone: "+15555550112",
      noPassword: true,
    });

    assert.equal(isUsernameTaken(withPass.username), true);
    assert.equal(isUsernameTaken("definitely-not-taken-xyz"), false);

    const ok = await authenticateAccount(withPass.username, "s3cret!");
    assert.ok(ok);
    assert.equal(ok.id, withPass.id);

    assert.equal(await authenticateAccount(withPass.username, "wrong"), null);
    assert.equal(await authenticateAccount(withPass.username, ""), null);
    assert.equal(await authenticateAccount("missing-user", "s3cret!"), null);

    const okEmpty = await authenticateAccount(noPass.username, "");
    assert.ok(okEmpty);
    assert.equal(okEmpty.id, noPass.id);
    assert.equal(await authenticateAccount(noPass.username, "x"), null);

    await setAccountPassword(noPass.id, "new-secret");
    assert.equal(accountHasNoPassword(noPass.id), false);
    assert.ok(await authenticateAccount(noPass.username, "new-secret"));
    assert.equal(await authenticateAccount(noPass.username, ""), null);

    await setAccountPassword(noPass.id, null);
    assert.equal(accountHasNoPassword(noPass.id), true);
    assert.ok(await authenticateAccount(noPass.username, ""));
    assert.equal(await authenticateAccount(noPass.username, "new-secret"), null);
  });
});
