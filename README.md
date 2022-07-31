# cc-taskboard-server

Сервер для работы приложения CC TaskBoard, предназначенного для отслеживания времени выполнения задач и составления графика работы на сутки и более.

Статус: `сформирован API для управления досками и их содержимым`.

## Компиляция и запуск

Для сборки введите команду:

```bash
$ cargo build --release
```

Перед запуском предварительно установите PostgreSQL. Создайте пользователя `cc-taskboard-server`, базу данных `cc-taskboard-server` и дайте привилегии пользователю на эту базу данных. После этого запустите PostgreSQL.

Для запуска сервера создайте в `target/release` файл `app_config.json`:

```json
{
  "pg": "host=... user='...' password='...' connect_timeout=10 keepalives=0",
  "admin_key": "...",
  "hyper_addr": "0.0.0.0:<порт>"
}
```

 и выполните:

```bash
$ ./cc-taskboard-server app_config.json
```

## API

Описания методов REST API находятся в файле [API.md](./API.md).

## Лицензия

Исходный код сервера опубликован по лицензии GNU General Public License третьей версии ([см. текст](./LICENSE)).
