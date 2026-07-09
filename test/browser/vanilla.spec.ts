import { expect, test } from '@playwright/test';

test('vanilla example starts and reports generated state', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Generated House Engine' })).toBeVisible();
  await page.locator('#seedInput').fill('777');
  await page.locator('#lengthInput').fill('00:05');
  await page.getByRole('button', { name: 'Start' }).click();
  await expect(page.getByText('Playing.')).toBeVisible({ timeout: 10000 });
  await expect(page.locator('#time')).toContainText('/ 00:05');
  await expect(page.locator('#section')).not.toHaveText('');
  await expect(page.locator('#seed')).toHaveText('777');

  await page.getByRole('button', { name: 'New Random' }).click();
  await expect(page.getByText('Playing.')).toBeVisible({ timeout: 10000 });
  await expect(page.locator('#seed')).not.toHaveText('777');
});

test('vanilla example switches profile while playing', async ({ page }) => {
  await page.goto('/');
  const profileInput = page.locator('#profileInput');

  await expect(profileInput.locator('option')).toHaveCount(3);
  await expect(profileInput).toHaveValue('amiga_house_95ish');

  await page.locator('#lengthInput').fill('00:05');
  await page.getByRole('button', { name: 'Start' }).click();
  await expect(page.getByText('Playing.')).toBeVisible({ timeout: 10000 });

  await profileInput.selectOption('dub_deep_house');
  await expect(profileInput).toHaveValue('dub_deep_house');
  await expect(page.getByText('Playing.')).toBeVisible();
});
