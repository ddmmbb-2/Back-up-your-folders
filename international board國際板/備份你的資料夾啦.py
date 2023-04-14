import os
import zipfile
import shutil
from datetime import datetime
from tkinter import Tk, Label, Button, Listbox, filedialog, IntVar, messagebox, Frame
from tkinter.ttk import Progressbar
from pathlib import Path
from tkinter import OptionMenu, StringVar


lang = 'EN'  # Change the value to switch between languages: 'EN', 'TC', 'SC'

# Load language strings from file
with open(f'lang_{lang}.txt', 'r', encoding='utf-8') as f:
    strings = f.read().splitlines()

def create_backup_folder():
    if not os.path.exists(strings[0]):
        os.mkdir(strings[0])

def get_total_size(files):
    total_size = 0
    for path in files:
        if path.is_file() and not str(path).startswith(strings[0]):
            total_size += path.stat().st_size
    return total_size

def backup_files():
    create_backup_folder()
    current_time = datetime.now().strftime("%Y%m%d-%H-%M-%S")
    backup_filename = f"{strings[0]}/backup-{current_time}.zip"

    files_to_backup = list(Path(".").rglob("*"))
    total_size = get_total_size(files_to_backup)
    progress_var.set(0)
    progressbar.config(maximum=len(files_to_backup))

    if total_size > 500 * 1024 * 1024:
        backup_status_label.config(text=strings[1])
    else:
        backup_status_label.config(text=strings[2])

    app.update()

    with zipfile.ZipFile(backup_filename, "w") as zip_file:
        for idx, path in enumerate(files_to_backup):
            if path.is_file() and not str(path).startswith(strings[0]):
                zip_file.write(str(path))

            progress_var.set(idx + 1)
            progressbar.config(value=idx + 1)
            progress_label.config(text=f"{idx + 1}/{len(files_to_backup)}")
            app.update()

    update_backup_list()
    backup_status_label.config(text=strings[3])

def update_backup_list():
    if not os.path.exists(strings[0]):
        create_backup_folder()

    listbox.delete(0, 'end')
    for file in os.listdir(strings[0]):
        if file.endswith(".zip"):
            listbox.insert('end', file)

def restore_backup():
    selected_file = listbox.get(listbox.curselection())
    backup_file = os.path.join(strings[0], selected_file)

    with zipfile.ZipFile(backup_file, "r") as zip_file:
        backup_status_label.config(text=strings[4]) # 還原中...
        app.update()
        zip_file.extractall()

    backup_status_label.config(text=strings[5])  # 還原完畢

def delete_backup():
    selected_file = listbox.get(listbox.curselection())
    backup_file = os.path.join(strings[0], selected_file)
    confirm = messagebox.askyesno(title=strings[6], message=f"{strings[7]} {selected_file}?")
    if confirm:
        os.remove(backup_file)
        update_backup_list()

app = Tk()
app.title(strings[8])
app.geometry("500x450")
app.resizable(0, 0)

label = Label(app, text=strings[9], pady=10)
label.pack()

def change_lang(*args):
    global lang, strings
    lang = lang_var.get()
    lang_code = {'EN': 'EN', '繁中': 'TC', '簡中': 'SC'}[lang]
    with open(f'lang_{lang_code}.txt', 'r', encoding='utf-8') as f:
        strings = f.read().splitlines()
    update_ui()
    update_backup_list()  # 在這裡添加更新備份列表的功能

def update_ui():
    # Update all UI components with the new language strings
    label.config(text=strings[9])
    backup_button.config(text=strings[10])
    restore_button.config(text=strings[11])
    delete_button.config(text=strings[12])
    description_label.config(text=strings[13])


# 在這裡添加下拉菜單和語言標籤
lang_frame = Frame(app)
lang_frame.pack()

lang_label = Label(lang_frame, text="語言 / Language:")
lang_label.pack(side='left')

lang_var = StringVar(app)
lang_var.set(lang)  # 預設語言
lang_var.trace("w", change_lang)

lang_options = ['EN', '繁中', '簡中']
lang_menu = OptionMenu(lang_frame, lang_var, *lang_options)
lang_menu.pack(side='left', padx=5)




# 建立按鈕容器
button_frame = Frame(app)
button_frame.pack(pady=5)

backup_button = Button(button_frame, text=strings[10], command=backup_files)
backup_button.pack(side='left', padx=5)

restore_button = Button(button_frame, text=strings[11], command=restore_backup)
restore_button.pack(side='left', padx=5)

delete_button = Button(button_frame, text=strings[12], command=delete_backup)
delete_button.pack(side='left', padx=5)

listbox = Listbox(app, width=60, height=10)
listbox.pack(pady=5)

update_backup_list()

backup_status_label = Label(app, text="")
backup_status_label.pack(pady=5)

progress_var = IntVar()
progress_var.set(0)
progressbar = Progressbar(app, variable=progress_var, mode="determinate", length=400)
progressbar.pack(pady=10)

progress_label = Label(app, text="")
progress_label.pack(pady=5)

description_label = Label(app, text=strings[13], wraplength=450, pady=10)
description_label.pack()

app.mainloop()
