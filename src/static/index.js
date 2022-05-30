var uploaderIdx = 0;
var breadcrumb, paths, readonly;
var $toolbox, $tbody, $breadcrumb, $uploaders, $uploadControl;

class Uploader {
  idx = 0;
  file;
  $elem;
  constructor(idx, file) {
    this.idx = idx;
    this.file = file;
  }

  upload() {
    var { file, idx } = this;
    var url = getUrl(file.name);
    $uploaders.insertAdjacentHTML("beforeend", `
  <div class="uploader path">
    <div><svg height="16" viewBox="0 0 12 16" width="12"><path fill-rule="evenodd" d="M6 5H2V4h4v1zM2 8h7V7H2v1zm0 2h7V9H2v1zm0 2h7v-1H2v1zm10-7.5V14c0 .55-.45 1-1 1H1c-.55 0-1-.45-1-1V2c0-.55.45-1 1-1h7.5L12 4.5zM11 5L8 2H1v12h10V5z"></path></svg></div>
    <a href="${url}" id="file${idx}">${file.name} (0%)</a>
  </div>`);
    this.$elem = document.getElementById(`file${idx}`);

    var ajax = new XMLHttpRequest();
    ajax.upload.addEventListener("progress", e => this.progress(e), false);
    ajax.addEventListener("load", e => this.complete(e), false);
    ajax.addEventListener("error", e => this.fail(e), false);
    ajax.addEventListener("abort", e => this.fail(e), false);
    ajax.open("PUT", url);
    ajax.send(file);
  }

  progress(event) {
    var percent = (event.loaded / event.total) * 100;
    this.$elem.innerHTML = `${this.file.name} (${percent.toFixed(2)}%)`;
  }

  complete(event) {
    this.$elem.innerHTML = `${this.file.name}`;
  }

  fail(event) {
    this.$elem.innerHTML = `<strike>${this.file.name}</strike>`;
  }
}

function addBreadcrumb(value) {
  var parts = value.split("/").filter(v => !!v);
  var len = parts.length;
  var path = "";
  for (var i = 0; i < len; i++) {
    var name = parts[i];
    if (i > 0) {
      path  += "/" + name;
    }
    if (i === len - 1) {
      $breadcrumb.insertAdjacentHTML("beforeend", `<b>${name}</b>`);
    } else if (i === 0) {
      $breadcrumb.insertAdjacentHTML("beforeend", `<a href="/"><b>${name}</b></a>`);
    } else {
      $breadcrumb.insertAdjacentHTML("beforeend", `<a href="${encodeURI(path)}">${name}</a>`);
    }
    $breadcrumb.insertAdjacentHTML("beforeend", `<span class="separator">/</span>`);
  }
}

function addPath(file, index) {
  var url = getUrl(file.name)
  var actionDelete = "";
  var actionDownload = "";
  if (file.path_type.endsWith("Dir")) {
    actionDownload = `
    <div class="action-btn">
      <a href="${url}?zip" title="Download folder as a .zip file">
        <svg width="16" height="16" viewBox="0 0 16 16"><path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/><path d="M7.646 11.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 10.293V1.5a.5.5 0 0 0-1 0v8.793L5.354 8.146a.5.5 0 1 0-.708.708l3 3z"/></svg>
      </a>
    </div>`;
  } else {
    actionDownload = `
    <div class="action-btn" >
      <a href="${url}" title="Download file" download>
        <svg width="16" height="16" viewBox="0 0 16 16"><path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/><path d="M7.646 11.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 10.293V1.5a.5.5 0 0 0-1 0v8.793L5.354 8.146a.5.5 0 1 0-.708.708l3 3z"/></svg>
      </a>
    </div>`;
  }
  if (!readonly) {
    actionDelete = `
    <div onclick="deletePath(${index})" class="action-btn" id="deleteBtn${index}" title="Delete ${file.name}">
      <svg width="16" height="16" fill="currentColor"viewBox="0 0 16 16"><path d="M6.854 7.146a.5.5 0 1 0-.708.708L7.293 9l-1.147 1.146a.5.5 0 0 0 .708.708L8 9.707l1.146 1.147a.5.5 0 0 0 .708-.708L8.707 9l1.147-1.146a.5.5 0 0 0-.708-.708L8 8.293 6.854 7.146z"/><path d="M14 14V4.5L9.5 0H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2zM9.5 3A1.5 1.5 0 0 0 11 4.5h2V14a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1h5.5v2z"/></svg>
    </div>`;
  }
  var actionCell = `
  <td class="cell-actions">
    ${actionDownload}
    ${actionDelete}
  </td>`

  $tbody.insertAdjacentHTML("beforeend", `
<tr id="addPath${index}">
<td class="path cell-name">
  <div>${getSvg(file.path_type)}</div>
  <a href="${url}" title="${file.name}">${file.name}</a>
</td>
<td class="cell-mtime">${formatMtime(file.mtime)}</td>
<td class="cell-size">${formatSize(file.size)}</td>
${actionCell}
</tr>`)
}

async function deletePath(index) {
  var file = paths[index];
  if (!file) return;

  var ajax = new XMLHttpRequest();
  ajax.open("DELETE", getUrl(file.name));
  ajax.addEventListener("readystatechange", function() {
    if(ajax.readyState === 4 && ajax.status === 200) {
        document.getElementById(`addPath${index}`).remove();
    }
  });
  ajax.send();
}

function addUploadControl() {
  $toolbox.insertAdjacentHTML("beforeend", `
<div class="upload-control" title="Upload file">
  <label for="file">
    <svg width="16" height="16" viewBox="0 0 16 16"><path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/><path d="M7.646 1.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1-.708.708L8.5 2.707V11.5a.5.5 0 0 1-1 0V2.707L5.354 4.854a.5.5 0 1 1-.708-.708l3-3z"/></svg>
  </label>
  <input type="file" id="file" name="file" multiple>
</div>
`);
}

function getUrl(name) {
    var url = location.href.split('?')[0];
    if (!url.endsWith("/")) url += "/";
    url += encodeURI(name);
    return url;
}

function getSvg(path_type) {
  switch (path_type) {
    case "Dir":
      return `<svg height="16" viewBox="0 0 14 16" width="14"><path fill-rule="evenodd" d="M13 4H7V3c0-.66-.31-1-1-1H1c-.55 0-1 .45-1 1v10c0 .55.45 1 1 1h12c.55 0 1-.45 1-1V5c0-.55-.45-1-1-1zM6 4H1V3h5v1z"></path></svg>`;
    case "File":
      return `<svg height="16" viewBox="0 0 12 16" width="12"><path fill-rule="evenodd" d="M6 5H2V4h4v1zM2 8h7V7H2v1zm0 2h7V9H2v1zm0 2h7v-1H2v1zm10-7.5V14c0 .55-.45 1-1 1H1c-.55 0-1-.45-1-1V2c0-.55.45-1 1-1h7.5L12 4.5zM11 5L8 2H1v12h10V5z"></path></svg>`;
    case "SymlinkDir":
      return `<svg height="16" viewBox="0 0 14 16" width="14"><path fill-rule="evenodd" d="M13 4H7V3c0-.66-.31-1-1-1H1c-.55 0-1 .45-1 1v10c0 .55.45 1 1 1h12c.55 0 1-.45 1-1V5c0-.55-.45-1-1-1zM1 3h5v1H1V3zm6 9v-2c-.98-.02-1.84.22-2.55.7-.71.48-1.19 1.25-1.45 2.3.02-1.64.39-2.88 1.13-3.73C4.86 8.43 5.82 8 7.01 8V6l4 3-4 3H7z"></path></svg>`;
    default:
      return `<svg height="16" viewBox="0 0 12 16" width="12"><path fill-rule="evenodd" d="M8.5 1H1c-.55 0-1 .45-1 1v12c0 .55.45 1 1 1h10c.55 0 1-.45 1-1V4.5L8.5 1zM11 14H1V2h7l3 3v9zM6 4.5l4 3-4 3v-2c-.98-.02-1.84.22-2.55.7-.71.48-1.19 1.25-1.45 2.3.02-1.64.39-2.88 1.13-3.73.73-.84 1.69-1.27 2.88-1.27v-2H6z"></path></svg>`;
  }
}

function formatMtime(mtime) {
  if (!mtime) return ""
  var date = new Date(mtime);
  var year = date.getFullYear();
  var month = padZero(date.getMonth() + 1, 2);
  var day = padZero(date.getDate(), 2);
  var hours = padZero(date.getHours(), 2);
  var minutes = padZero(date.getMinutes(), 2);
  return `${year}/${month}/${day} ${hours}:${minutes}`;
}

function padZero(value, size) {
  return ("0".repeat(size) + value).slice(-1 * size)
}

function formatSize(size) {
  if (!size) return ""
  var sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  if (size == 0) return '0 Byte';
  var i = parseInt(Math.floor(Math.log(size) / Math.log(1024)));
  return Math.round(size / Math.pow(1024, i), 2) + ' ' + sizes[i];
}


function ready() {
  breadcrumb = DATA.breadcrumb;
  paths = DATA.paths;
  readonly = DATA.readonly;
  $toolbox = document.querySelector(".toolbox");
  $tbody = document.querySelector(".main tbody");
  $breadcrumb = document.querySelector(".breadcrumb");
  $uploaders = document.querySelector(".uploaders");
  $uploadControl = document.querySelector(".upload-control");

  addBreadcrumb(breadcrumb);
  paths.forEach((file, index) => addPath(file, index));
  if (!readonly) {
    addUploadControl();
    document.getElementById("file").addEventListener("change", e => {
      var files = e.target.files;
      for (var file of files) {
        uploaderIdx += 1;
        var uploader = new Uploader(uploaderIdx, file);
        uploader.upload();
      }
    });
  }
}