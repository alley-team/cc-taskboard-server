#!/usr/bin/env python

import json
import base64

task = {
  "login": input("Введите логин: "),
  "pass": input("Введите пароль (не менее 8 символов): "),
  "cc_key": input("Введите ключ регистрации: ")
}

s = json.dumps(task)
token = base64.b64encode(bytes(s, 'utf-8'))

print(token)
