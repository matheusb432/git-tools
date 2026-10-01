use anyhow::{Result, ensure};
use thirtyfour::{By, Key, WebDriver};

use crate::support::{self, fixture::OneShotFixture};

#[tokio::test(flavor = "multi_thread")]
async fn saved_viewer_settings_apply_and_survive_restart() -> Result<()> {
    support::run_test("settings", |session| {
        Box::pin(async move {
            let fixture = OneShotFixture::create_named(session.data_root(), "settings-review")?;
            fixture.forward()?;
            let driver = session.driver();
            support::wait_for_active_diff(driver, "settings-review", "alpha-one-shot-marker")
                .await?;

            open_settings(driver).await?;
            observe_input_stability(driver).await?;
            select_value(driver, "settings-theme", "glacier").await?;
            select_value(driver, "settings-theme", "mirage").await?;
            select_value(driver, "settings-theme", "carbon").await?;
            support::evidence::capture(driver, "settings-appearance", true).await?;
            support::click(driver, By::Css("[data-settings-section='snapshots']")).await?;
            ensure!(driver.current_url().await?.path().ends_with("/settings/snapshots"), "settings category did not update the route");
            choose_radio(driver, "settings-layout", "split").await?;
            choose_radio(driver, "settings-density", "full").await?;
            support::click(driver, By::Css("#settings-wrap-lines")).await?;
            support::click(driver, By::Css("#settings-copy-with-line-context")).await?;
            support::click(driver, By::Css("#settings-wrap-lines")).await?;
            support::click(driver, By::Css("#settings-wrap-lines")).await?;
            support::evidence::capture(driver, "settings-snapshots", true).await?;
            support::click(driver, By::Css("[data-settings-section='locale']")).await?;
            choose_radio(driver, "settings-language", "pt-BR").await?;
            choose_radio(driver, "settings-language", "en-US").await?;
            choose_radio(driver, "settings-language", "pt-BR").await?;
            support::visible(driver, By::Css("html[data-theme='carbon'][lang='pt-BR']")).await?;

            support::click(driver, By::Css("#settings-source-link")).await?;
            support::visible(driver, By::Css("#settings-source pre")).await?;
            support::click(
                driver,
                By::Css("button[aria-label='Fechar Arquivo de configuração']"),
            )
            .await?;
            support::evidence::capture(driver, "settings-locale", true).await?;
            support::click(driver, By::Css("button[aria-label='Voltar']")).await?;
            support::visible(
                driver,
                By::Css("[data-gtl-diff-document][data-layout='split'][data-density='full']"),
            )
            .await?;
            let copied = support::copy_selected_diff_line(driver, "work.txt", "alpha-one-shot-marker").await?;
            ensure!(copied == "alpha-one-shot-marker", "plain diff copy included context: {copied:?}");
            support::click(driver, By::Css("button[aria-label='Configurações']")).await?;
            let language = support::visible(driver, By::Css("input[name='settings-language'][value='pt-BR']")).await?;
            ensure!(language.is_selected().await?, "returning to settings lost the selected language");
            let changes = driver.execute("cancelAnimationFrame(window.settingsObservation.frame); return window.settingsObservation.regressions;", Vec::new()).await?.convert::<Vec<String>>()?;
            ensure!(changes.is_empty(), "settings reverted after input: {changes:?}");
            session.restart().await?;
            let driver = session.driver();
            support::visible(driver, By::Css("html[data-theme='carbon'][lang='pt-BR']")).await?;
            fixture.forward()?;
            support::wait_for_active_diff(driver, "settings-review", "alpha-one-shot-marker")
                .await?;
            support::visible(
                driver,
                By::Css("[data-gtl-diff-document][data-layout='split'][data-density='full']"),
            )
            .await?;
            let copied = support::copy_selected_diff_line(driver, "work.txt", "alpha-one-shot-marker").await?;
            ensure!(copied == "alpha-one-shot-marker", "restarted viewer did not retain plain diff copy: {copied:?}");
            Ok(())
        })
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn keyboard_shortcuts_are_editable_and_survive_restart() -> Result<()> {
    support::run_test("settings-keybindings", |session| {
        Box::pin(async move {
            let fixture = OneShotFixture::create_named(session.data_root(), "shortcut-review")?;
            fixture.forward()?;
            let driver = session.driver();
            support::wait_for_active_diff(driver, "shortcut-review", "alpha-one-shot-marker")
                .await?;
            open_settings(driver).await?;
            support::click(driver, By::Css("[data-settings-section='keybindings']")).await?;
            let search = support::visible(driver, By::Id("keybindings-search")).await?;
            search.send_keys("toggle files").await?;
            support::click(driver, By::Id("keybinding-edit-toggle_files_sidebar")).await?;
            support::visible(driver, By::Css("#keybinding-recorder[open]")).await?;
            press_shortcut(driver, Key::Control, "p").await?;
            support::visible(driver, By::Id("keybinding-replace")).await?;
            support::evidence::capture(driver, "keybindings-conflict", true).await?;
            driver
                .action_chain()
                .send_keys(Key::Escape)
                .perform()
                .await?;
            support::visible(driver, By::Css(".settings-page-shell:not(:has(dialog[open])) #keybinding-edit-toggle_files_sidebar")).await?;
            support::click(driver, By::Id("keybinding-edit-toggle_files_sidebar")).await?;
            support::visible(driver, By::Css("#keybinding-recorder[open]")).await?;
            press_shortcut(driver, Key::Alt, "x").await?;
            support::click(driver, By::Id("keybinding-save")).await?;
            support::visible(driver, By::Css("#keybinding-edit-toggle_files_sidebar[aria-label$='Alt+X']")).await?;
            support::click(driver, By::Css("button[aria-label='Clear search']")).await?;
            support::evidence::capture(driver, "keybindings-settings", true).await?;
            driver.set_window_rect(100, 100, 760, 650).await?;
            support::evidence::capture(driver, "keybindings-settings-narrow", true).await?;
            driver.set_window_rect(100, 100, 1280, 900).await?;
            support::click(driver, By::Id("keybindings-record-search")).await?;
            press_shortcut(driver, Key::Alt, "x").await?;
            support::visible(driver, By::Id("keybinding-edit-toggle_files_sidebar")).await?;
            ensure!(
                driver
                    .find_all(By::Css("[data-keybinding-action]"))
                    .await?
                    .len()
                    == 1,
                "recorded search did not find the assigned command"
            );
            support::click(driver, By::Css("button[aria-label='Back']")).await?;
            support::visible(driver, By::Css("[data-sidebar-panel='files']")).await?;
            press_shortcut(driver, Key::Alt, "x").await?;
            support::visible(
                driver,
                By::Css("[data-sidebar-toggle='files'][aria-pressed='false']"),
            )
            .await?;
            press_shortcut(driver, Key::Alt, "x").await?;
            support::visible(driver, By::Css("[data-sidebar-panel='files']")).await?;
            session.restart().await?;
            let driver = session.driver();
            fixture.forward()?;
            support::wait_for_active_diff(driver, "shortcut-review", "alpha-one-shot-marker")
                .await?;
            press_shortcut(driver, Key::Alt, "x").await?;
            support::visible(
                driver,
                By::Css("[data-sidebar-toggle='files'][aria-pressed='false']"),
            )
            .await?;
            open_settings(driver).await?;
            support::click(driver, By::Css("[data-settings-section='keybindings']")).await?;
            support::click(
                driver,
                By::Css("button[aria-label='Remove shortcut for View: Toggle Files sidebar']"),
            )
            .await?;
            let binding =
                support::visible(driver, By::Id("keybinding-edit-toggle_files_sidebar")).await?;
            ensure!(
                binding.text().await? == "Unassigned",
                "removed shortcut stayed assigned"
            );
            support::click(driver, By::Id("keybindings-reset-all")).await?;
            support::click(driver, By::Css("button[aria-label='Back']")).await?;
            press_shortcut(driver, Key::Control, "b").await?;
            support::visible(driver, By::Css("[data-sidebar-panel='files']")).await?;
            Ok(())
        })
    })
    .await
}

async fn press_shortcut(driver: &WebDriver, modifier: Key, key: &str) -> Result<()> {
    driver
        .action_chain()
        .key_down(modifier.clone())
        .send_keys(key)
        .key_up(modifier)
        .perform()
        .await?;
    Ok(())
}

pub(super) async fn open_settings(driver: &WebDriver) -> Result<()> {
    support::click(driver, By::Css("button[aria-label='Settings']")).await?;
    support::visible(driver, By::Css("#settings-theme")).await?;
    Ok(())
}

pub(super) async fn select_value(driver: &WebDriver, id: &str, value: &str) -> Result<()> {
    support::click(driver, By::Id(id)).await?;
    support::click(
        driver,
        By::Css(format!(
            "#{id}-listbox [role='option'][data-value='{value}']"
        )),
    )
    .await?;
    support::visible(driver, By::Css(format!("#{id}[value='{value}']"))).await?;
    Ok(())
}

async fn choose_radio(driver: &WebDriver, name: &str, value: &str) -> Result<()> {
    support::click(
        driver,
        By::Css(format!(
            "input[type='radio'][name='{name}'][value='{value}']"
        )),
    )
    .await
}

// Observe native control values at paint boundaries; interactions still use WebDriver.
async fn observe_input_stability(driver: &WebDriver) -> Result<()> {
    driver.execute(r#"
        const observation = { expected: new Map(), regressions: [], layout: null, frame: null };
        window.settingsObservation = observation;
        function expect(input, key, value) {
            observation.layout = input.name === 'settings-language' ? null : {
                route: location.pathname,
                hint: document.querySelector('.settings-save-status')?.textContent,
                rows: [...document.querySelectorAll('.settings-field-row')].map(row => row.getBoundingClientRect().y),
            };
            observation.expected.set(key, {
                radio: input.type === 'radio',
                checkbox: input.type === 'checkbox',
                value,
            });
        }
        document.addEventListener('change', event => {
            const input = event.target;
            if (!input.closest('.settings-page-shell')) return;
            const key = input.type === 'radio' ? input.name : input.id;
            expect(input, key, input.type === 'checkbox' ? input.checked : input.value);
        }, true);
        document.addEventListener('click', event => {
            const option = event.target.closest?.('[role="option"][data-value]');
            const listbox = option?.closest('[role="listbox"]');
            if (!listbox?.closest('.settings-page-shell')) return;
            const trigger = document.querySelector(`[aria-controls="${listbox.id}"]`);
            if (trigger) expect(trigger, trigger.id, option.dataset.value);
        }, true);
        function sample() {
            for (const [key, expected] of observation.expected) {
                const input = expected.radio
                    ? document.querySelector(`input[name="${key}"]:checked`)
                    : document.getElementById(key);
                if (!input) continue;
                const value = expected.checkbox ? input.checked : input.value;
                if (value !== expected.value && observation.regressions.length < 32) {
                    observation.regressions.push(`${key}: expected ${expected.value}, displayed ${value}`);
                }
            }
            const layout = observation.layout;
            if (layout && layout.route === location.pathname) {
                const rows = [...document.querySelectorAll('.settings-field-row')];
                const moved = rows.some((row, i) => Math.abs(row.getBoundingClientRect().y - layout.rows[i]) > 1);
                const hint = document.querySelector('.settings-save-status')?.textContent;
                if ((moved || hint !== layout.hint) && observation.regressions.length < 32) {
                    observation.regressions.push('local save changed the settings layout or status message');
                }
            } else {
                observation.layout = null;
            }
            if (location.pathname.startsWith('/settings/') && document.querySelector('.settings-page-shell')) {
                const tabs = document.querySelector('[role="tablist"]');
                if (tabs?.getClientRects().length && observation.regressions.length < 32) {
                    observation.regressions.push('workspace tabs remained visible on the settings route');
                }
            }
            observation.frame = requestAnimationFrame(sample);
        }
        sample();
    "#, Vec::new()).await?;
    Ok(())
}
