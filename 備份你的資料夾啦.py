import os
import zipfile
import shutil
from datetime import datetime
from tkinter import Tk, Label, Button, Listbox, filedialog, IntVar, messagebox, Frame
from tkinter.ttk import Progressbar
from pathlib import Path

def create_backup_folder():
    if not os.path.exists("備份"):
        os.mkdir("備份")

def get_total_size(files):
    total_size = 0
    for path in files:
        if path.is_file() and not str(path).startswith("備份"):
            total_size += path.stat().st_size
    return total_size

def backup_files():
    create_backup_folder()
    current_time = datetime.now().strftime("%Y%m%d-%H-%M-%S")
    backup_filename = f"備份/backup-{current_time}.zip"

    files_to_backup = list(Path(".").rglob("*"))
    total_size = get_total_size(files_to_backup)
    progress_var.set(0)
    progressbar.config(maximum=len(files_to_backup))

    if total_size > 500 * 1024 * 1024:
        backup_status_label.config(text="備份中... 你的檔案很大，非常大。備份中請勿關閉程式\n有耐心的等候備份完成。程式看起來當掉是很正常的。")
    else:
        backup_status_label.config(text="備份中... 請等待備份完成")

    app.update()

    with zipfile.ZipFile(backup_filename, "w") as zip_file:
        for idx, path in enumerate(files_to_backup):
            if path.is_file() and not str(path).startswith("備份"):
                zip_file.write(str(path))

            progress_var.set(idx + 1)
            progressbar.config(value=idx + 1)
            progress_label.config(text=f"{idx + 1}/{len(files_to_backup)}")
            app.update()

    update_backup_list()
    backup_status_label.config(text="備份完成")

def update_backup_list():
    if not os.path.exists("備份"):
        create_backup_folder()

    listbox.delete(0, 'end')
    for file in os.listdir("備份"):
        if file.endswith(".zip"):
            listbox.insert('end', file)

def restore_backup():
    selected_file = listbox.get(listbox.curselection())
    backup_file = os.path.join("備份", selected_file)

    with zipfile.ZipFile(backup_file, "r") as zip_file:
        zip_file.extractall()

def delete_backup():
    selected_file = listbox.get(listbox.curselection())
    backup_file = os.path.join("備份", selected_file)
    confirm = messagebox.askyesno(title="刪除確認", message=f"您確定要刪除 {selected_file} 嗎？")
    if confirm:
        os.remove(backup_file)
        update_backup_list()

app = Tk()
app.title("備份你的資料夾啦")
app.geometry("500x450")
app.resizable(0, 0)

label = Label(app, text="檔案備份與還原", pady=10)
label.pack()

# 建立按鈕容器
button_frame = Frame(app)
button_frame.pack(pady=5)

backup_button = Button(button_frame, text="備份檔案", command=backup_files)
backup_button.pack(side='left', padx=5)

restore_button = Button(button_frame, text="還原檔案", command=restore_backup)
restore_button.pack(side='left', padx=5)

delete_button = Button(button_frame, text="刪除檔案", command=delete_backup)
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

description_label = Label(app, text="這個程式用於備份和恢復當前資料夾中的檔案。請使用上方的按鈕進行操作。", wraplength=450, pady=10)
description_label.pack()

app.mainloop()
