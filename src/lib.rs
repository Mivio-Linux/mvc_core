use std::{fs::{self, read}, io::{self, Write}, path::{Path, PathBuf}};
use serde_json::{Value};
use tar::{Archive, Builder, Header};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

  //            //
 // CONSTANTES //
//            //
// Paths
pub const SNAP_METADATA_PATH: &str = "metadata";
pub const SNAP_ARCHIVE_PATH: &str = "archives";
const HEAD_PATH: &str = "HEAD";
  //            //
 // STRUCTURES //
//            //
#[derive(Serialize, Deserialize, Debug)]
pub struct Snapshot {
    /// Archive hash
    pub hash: String, 
    /// Commit message
    pub message: String, 
    /// email of the user who made the commit
    pub email: String, 
    /// name of the user who made the commit
    pub name: String, 
}
#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub email: String,
    pub name: String,
}
pub struct Repository {
    path: PathBuf
}
  //                 //
 // IMPLEMENTATIONS //
//                 //
impl User {
    /// create an user from name:
    /// ```
    /// use mvc_core::User;
    /// let user = User::new_from_name("My name");
    /// // User {
    /// // name: "My name"
    /// // email: "None" 
    /// //}
    /// ```
    pub fn new_from_name(name:&str) -> User {
        User { email: "None".to_string(), name: name.to_string()}
    }
        /// create an user from email:
    /// ```
    /// use mvc_core::User;
    /// let user = User::new_from_email("My@mail.foo");
    /// // User {
    /// // name: "None"
    /// // email: "My@mail.foo" 
    /// //}
    /// ```
    pub fn new_from_email(email:&str) -> User {
        User { email: email.to_string(), name: "None".to_string() }
    }
}
impl Repository {
    /// create a new repository (not init!):
    /// ```
    /// use mvc_core::Repository;
    /// let repo = Repository::new("path/to/repository");
    /// ```
    pub fn new(path: &str) -> Repository{
        Repository {
            path: PathBuf::from(path)
        }
    }
    /// initialize repository (create an dirs, HEAD):
    /// ```
    /// use mvc_core::Repository;
    /// let repo = Repository::new("path/to/repository");
    /// repo.init(); // initialize repo 
    /// ```
    pub fn init(&self) -> Result<(), io::Error> {
        fs::create_dir_all(&self.path.join(SNAP_ARCHIVE_PATH))?; // создаем папки
        fs::create_dir_all(&self.path.join(SNAP_METADATA_PATH))?;
        let mut head = fs::File::create(&self.path.join(HEAD_PATH))?;
        head.write("0".as_bytes())?;
        Ok(())
    }
    /// save snapshot (not save files in ignore):
    /// ```
    /// use mvc_core::{Repository, User};
    /// let repo = Repository::new("path/to/repository");
    /// repo.init(); // initialize repo
    /// repo.save_snapshot("initial", vec![".mvc".to_string(),".mvcignore".to_string()], 
    /// &User{
    /// name: "name".to_string(),
    /// email: "new@email".to_string()},
    ///  ".").expect("ERR"); // pack_path: archives all child files of the directory "."
    /// ```
    pub fn save_snapshot(&self, message: &str, ignore: Vec<String>, user: &User, search_path: &str) -> Result<(), io::Error> {
            let last_snap = parse_last_snap_id(&self.path)?;
            let last_snap = last_snap + 1; 
            create_snap(last_snap, message, ignore, user, &self.path, search_path)?;
            fs::write(&self.path.join(HEAD_PATH), last_snap.to_string())?;
        Ok(())
    } 
    fn delete_current(&self, path: &Path, ignore: &Vec<String>) -> Result<(), std::io::Error>  {
    let current_dir = walkdir::WalkDir::new(path).min_depth(1).contents_first(true); // читаем текущую директорию
    
    for entry in current_dir {
        let borrowed_entry = entry?; 
        let should_ignore = !should_ignore(&borrowed_entry.path().strip_prefix("./").ok()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData,"Cant strip prefix ./ . line 113"))?,
        ignore);
        if should_ignore {
            if borrowed_entry.metadata()?.is_dir() {
                self.delete_current(borrowed_entry.path(), ignore)?;
            } else {
                fs::remove_file(borrowed_entry.path())?;
            }
        }
    }
    Ok(())
}
    /// return to the snapshot by id:
    /// ```
    /// use mvc_core::Repository;
    /// let repo = Repository::new("path/to/repository");
    /// repo.init(); // initialize repo
    /// repo.return_snapshot(1, ".").expect("ERR"); // unpack_path: unpacks files into this directory
    /// ```
    pub fn return_snapshot(&self, id: u32, unpack_path: &PathBuf, ignore_list: &Vec<String>) -> Result<String, io::Error> {
        let mut id_str = id.to_string();
        id_str.push_str(".tar");
        let binding = self.path.join(SNAP_ARCHIVE_PATH).join(id_str);
        let path = binding;
        let new_hash = calculate_hash(&path)?;
        // получаем его метаданные
        let mut id_str = id.to_string();
        id_str.push_str(".json");
        let binding = &self.path.join(SNAP_METADATA_PATH).join(id_str);
        let path = binding;
        let metadata: String = fs::read_to_string(path)?;
        let metadata: Value = serde_json::from_str(&metadata)?;
        if metadata["hash"].as_str().ok_or_else(||io::Error::new(io::ErrorKind::Other, 
            format!("[ERROR] Failed to convert serde_json::Value::String to &str")))?.to_string() != new_hash {
            return Err(std::io::Error::new(io::ErrorKind::Other, "Hashs not match"))
        }
        // очищаем директорию...
        self.delete_current(unpack_path, ignore_list)?;
        // распаковываем архив...
        unpack_arch(&self.path, &id, unpack_path)?;
        // выводим сообщение:
        Ok(metadata["message"].as_str().ok_or_else(||io::Error::new(io::ErrorKind::Other, 
            format!("[ERROR] Failed to convert serde_json::Value::String to &str")))?.to_string())
    }
}
  //           //
 // FUNCTIONS //
//           //
/// проверяет, надо ли игнорировать путь
fn should_ignore(path: &Path, ignore_list: &Vec<String>) -> bool{
    if !path.is_absolute() { // если путь не абсолютный
        let ancestors = path.ancestors(); // Создает итератор по объекту Path и его предкам.
        
        for ancestor in ancestors { // берем все элементы
            if ancestor == Path::new(".") {return true} // если путь это . (текущая директория) то игнорируй
            if ignore_list.iter().any(|ignore| {
                let ignore_path = &Some(ignore.as_str()) ==&ancestor.as_os_str().to_str();// если в игнор листе будет наш путь
                return ignore_path;
            }){
                return true; // то возвращаем true (да, игнорировать)
            }
        }
    } else {
        return true; // если путь будет абсолютным то есть риск удалить системные файлы, поэтому лучше будет игнорить
    }
    return false; // ну а если вапще чета как та и не то и не другое то false
}

fn create_snap(snap_id: u32, message: &str, ignore: Vec<String>, user: &User, path:&PathBuf, search_path: &str) -> Result<(), std::io::Error> {
        let mut id_str = snap_id.to_string();
        id_str.push_str(".tar");
        let binding = &path.join(SNAP_ARCHIVE_PATH).join(id_str);
        let path_arch = binding;
        create_archive(&fs::File::create(&path_arch)?, ignore, search_path)?;
        let hash = calculate_hash(&path_arch)?;
        // let user = get_user()?;
        let snapshot = Snapshot{
            hash: hash.to_string(),
            message: message.to_string(),
            email: user.email.clone(),
            name: user.name.clone()
        };
        let json_format: String = serde_json::to_string(&snapshot)?;
        let mut id_str = snap_id.to_string();
        id_str.push_str(".json");
        let binding = path.join(SNAP_METADATA_PATH).join(id_str);
        let path = binding;
        let mut info = fs::File::create(path)?;
        info.write(json_format.as_bytes())?;
        Ok(())
    }
fn calculate_hash(path: &PathBuf) -> Result<String, std::io::Error>  {
    let mut file = fs::File::open(path)?; // открываем файл
    let mut hasher = Sha256::new(); // создаем хешер
    io::copy(&mut file, &mut hasher)?; // копируем контент из file в hasher
    Ok(format!("{:x}", hasher.finalize())) // возвращаеем форматируя как строку и заканчивая хеш
}
/// Функция создающая архив с кодом
fn create_archive(arch_file: &fs::File, ignore: Vec<String>, search_path: &str) -> Result<(), io::Error> {
    let mut archive = Builder::new(arch_file);
    let read_dir = walkdir::WalkDir::new(search_path).min_depth(1).contents_first(false).follow_links(false);
    for object in read_dir {
        println!("{:?}", object);
        let object = object?; 
        let object_path = object.path().strip_prefix("./").unwrap_or(object.path());
        println!("{:?} not in ignore: {}", object_path, should_ignore(object_path, &ignore));
        println!("{:?}", ignore);
        
        if !should_ignore(object_path, &ignore) {
            if object.path_is_symlink() {
                let mut header = Header::new_gnu();
                header.set_entry_type(tar::EntryType::Symlink);
                let target = fs::read_link(object_path)?;
                archive.append_link(&mut header, object_path, &target)?; //FIXME
            }
            else if object.metadata()?.is_dir() { //проверь директория ли это
                archive.append_dir(&object_path, &object_path)?; // и заархивируй ее без дочерних элементами
                println!("{:?}", object.path().to_str().unwrap());
            }
            else { 
                archive.append_path(object_path)?; // просто заархивируй.
            }
        }
    }
    Ok(())
}
/// распаковка архива с данными по id
fn unpack_arch(path:&PathBuf, id: &u32, unpack_path: &PathBuf) -> Result<(), std::io::Error> {
    // delete_current(&Path::new("."))?;
    let mut id_str = id.to_string();
    id_str.push_str(".tar");
    let path = path.join(SNAP_ARCHIVE_PATH).join(id_str);
    let file = fs::File::open(path)?;
    let mut archive = Archive::new(file);
    archive.unpack(unpack_path)?;
    Ok(())
}
/// Парсим номер последнего снапшота
fn parse_last_snap_id(path: &PathBuf) -> Result<u32, io::Error> {
    let head_content = fs::read_to_string(path.join(HEAD_PATH))?; // читаем head
    let head_massive: Vec<&str> = head_content.split("\n").collect(); // разделяем на строки
    let last_snap_str = head_massive[0]; // берем первую строку (номер послед. коммита)
    let last_snap_int: u32 = last_snap_str.parse().ok().ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Unable to parse"))?;
    return Ok(last_snap_int);
}