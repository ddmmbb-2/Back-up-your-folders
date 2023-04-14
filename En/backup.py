import os
import zipfile
import shutil
from datetime import datetime
from tkinter import Tk, Label, Button, Listbox, filedialog, IntVar, messagebox, Frame
from tkinter.ttk import Progressbar
from pathlib import Path

def create_backup_folder():
    if not os.path.exists("Backup"):
        os.mkdir("Backup")

def get_total_size(files):
    total_size = 0
    for path in files:
        if path.is_file() and not str(path).startswith("Backup"):
            total_size += path.stat().st_size
    return total_size

def backup_files():
    create_backup_folder()
    current_time = datetime.now().strftime("%Y%m%d-%H-%M-%S")
    backup_filename = f"Backup/backup-{current_time}.zip"

    files_to_backup = list(Path(".").rglob("*"))
    total_size = get_total_size(files_to_backup)
    progress_var.set(0)
    progressbar.config(maximum=len(files_to_backup))

    if total_size > 500 * 1024 * 1024:
        backup_status_label.config(text="Backing up... Your files are very large, extremely large. Please be patient and do not close the program while backing up. It's normal if the program appears to crash.")
    else:
        backup_status_label.config(text="Backing up... Please wait for the backup to complete.")

    app.update()

    with zipfile.ZipFile(backup_filename, "w") as zip_file:
        for idx, path in enumerate(files_to_backup):
            if path.is_file() and not str(path).startswith("Backup"):
                zip_file.write(str(path))

            progress_var.set(idx + 1)
            progressbar.config(value=idx + 1)
            progress_label.config(text=f"{idx + 1}/{len(files_to_backup)}")
            app.update()

    update_backup_list()
    backup_status_label.config(text="Backup completed.")

def update_backup_list():
    if not os.path.exists("Backup"):
        create_backup_folder()

    listbox.delete(0, 'end')
    for file in os.listdir("Backup"):
        if file.endswith(".zip"):
            listbox.insert('end', file)

def restore_backup():
    selected_file = listbox.get(listbox.curselection())
    backup_file = os.path.join("Backup", selected_file)

    with zipfile.ZipFile(backup_file, "r") as zip_file:
        zip_file.extractall()

def delete_backup():
    selected_file = listbox.get(listbox.curselection())
    backup_file = os.path.join("Backup", selected_file)
    confirm = messagebox.askyesno(title="Deletion confirmation", message=f"Are you sure you want to delete {selected_file}?")
    if confirm:
        os.remove(backup_file)
        update_backup_list()

app = Tk()
app.title("backup_file")
app.geometry("500x450")
app.resizable(0, 0)

label = Label(app, text="Backup Your Folder", pady=10)
label.pack()

# 建立按鈕容器
button_frame = Frame(app)
button_frame.pack(pady=5)

backup_button = Button(button_frame, text="Backup Files", command=backup_files)
backup_button.pack(side='left', padx=5)

restore_button = Button(button_frame, text="Restore Files", command=restore_backup)
restore_button.pack(side='left', padx=5)

delete_button = Button(button_frame, text="Delete Files", command=delete_backup)
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

description_label = Label(app, text="This program is used to backup and restore files in the current folder. Please use the buttons above。", wraplength=450, pady=10)
description_label.pack()

app.mainloop()
