# cc-taskboard-server

Сервер для работы приложения CC TaskBoard, предназначенного для отслеживания времени выполнения задач и составления графика работы на сутки и более.

Статус: `управление досками, аутентификация, проверка оплаты`.

## Компиляция и запуск

Для сборки введите команду:

```bash
cargo build --release
```

Для запуска перейдите в `target/release` и введите:

```bash
./cc-taskboard-server
```

Или, если у вас есть файл конфигурации:

```json
{
  "pg": "host=... user='...' password='...' connect_timeout=10 keepalives=0",
  "admin_key": "...",
  "hyper_port": 8004
}
```

```bash
./cc-taskboard-server app_config.json
```

## API

Описания методов REST API находятся в файле [API.md](./API.md).

## Лицензия

Исходный код сервера опубликован по лицензии GNU General Public License третьей версии ([см. текст](./LICENSE)).
