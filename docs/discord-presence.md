# Иконка Termix в Discord (Rich Presence)

Знак `?` в Discord означает: Discord увидел неизвестный `termix.exe`
и у него нет иконки для него. Иконка `.ico`/ярлыка тут не помогает —
Discord берёт картинку только из своего Developer Portal по
`Application ID` через Rich Presence, который теперь встроен в Termix.

## Что уже сделано в коде (этот PR)

- `src-tauri/src/presence.rs` — фоновый поток: подключается к Discord IPC,
  ставит активность `SSH-терминал и SFTP / Работает в Termix` с большой
  картинкой `termix_logo`, обновляет раз в 30 сек, при закрытом Discord
  молча ретраит раз в 15 сек. Отключение: `TERMIX_NO_DISCORD=1`.
- `Application ID` вшит: `1551851692566380564`.
- `public/termix-logo-1024.png` (+512/256) — сгенерированы из
  `public/termix-brand.svg` через `sharp`, это исходник для загрузки
  в портал (Discord принимает только PNG/JPG, SVG нельзя).

## Осталось руками (5 минут, один раз)

Картинку в Discord загружает человек через портал — API для этого нет:

1. Открыть <https://discord.com/developers/applications/1551851692566380564/rich-presence/assets>
2. `Rich Presence → Art Assets → Add Image`
3. Загрузить `public/termix-logo-1024.png`, имя дать ровно **`termix_logo`**
4. Подождать 2–5 минут (кеш Discord)

Без шага 3 будет показываться текст активности, но вместо логотипа —
пустое место: имя ассета в коде обязано совпасть с именем в портале.

## Проверка после сборки

1. Нужен Discord Desktop (в браузере Rich Presence не работает).
2. `Настройки → Игровая активность → Отображать текущую активность` — вкл.
3. Установить Termix из `Termix_*_x64-setup.exe` (Action `Release installer`),
   запустить через ярлык из `AppData\Local\Termix` — через 10–15 сек
   в профиле появится `Termix` с логотипом. Если нет — `Ctrl+R` в Discord.
