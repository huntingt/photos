import $ from "jquery";

class Selections {
    constructor(core) { 
        this.core = core;
        this.lastSelected = [0, -1, null];
        this.selected = new Map();

        this.toggleHandle = e => this.toggle(e);
    }

    has(section, image=null) {
        if (this.selected.has(section)) {
            const val = this.selected.get(section);
            if (val === true || val.has(image)) {
                return true;
            }
        }
        return false;
    }

    clear() {
        this.selected = new Map();
        for (let elem of this.core.page.getElementsByClassName("selector"))
            elem.classList.remove("selected");
    }

    createHeaderToggle(section) {
        let elem = document.createElement('div');
        if (this.has(section)) {
            elem.className = "selector selected";
        } else {
            elem.className = "selector";
        }
        elem.addEventListener("click", e => this.toggleHeader(e, section));
        return elem;
    }
    
    toggleHeader(e, section) {
        const handle = e.currentTarget;
        const current = [section, -1, this.core.bind[section].data];

        if (e.shiftKey) {
            document.getSelection().removeAllRanges();
            const value = !handle.classList.contains("selected");
            this.rangeSet(this.lastSelected, current, value);
        } else {
            this._toggleHeader(e.currentTarget, section);
        }

        this.lastSelected = current;
    }

    _toggleHeader(handle, section) {
        const value = !handle.classList.contains("selected");
        this._setHeader(handle.parentNode.parentNode, section, value);
    }
    
    _setHeader(container, section, value) {
        this.selected.set(section, value || new Set());
        if (container) {
            let elems = container.getElementsByClassName("selector");
            for (let elem of elems) {
                if (value)  elem.classList.add("selected");
                else        elem.classList.remove("selected");
            }
        }
    }

    createToggle(section, image) {
        let elem = document.createElement('div');
        if (this.has(section, image)) {
            elem.className = "selector selected";
        } else {
            elem.className = "selector";
        }
        elem.addEventListener("click", this.toggleHandle);
        return elem;
    }
    
    toggle(e) {
        const handle = e.currentTarget;
        const section = handle.parentNode._section;
        const current = [section, handle.parentNode._index, this.core.bind[section].data];

        if (e.shiftKey) {
            document.getSelection().removeAllRanges();
            const value = !handle.classList.contains("selected");
            this.rangeSet(this.lastSelected, current, value);
        } else {
            this._toggle(handle, section);
        }
        
        this.lastSelected = current;
    }

    _toggle(handle, section) {
        this.openSelectFor(section);

        let set = this.selected.get(section);
        let cl = handle.classList;
        const img = handle.parentNode._src;

        if (cl.contains("selected")) {
            set.delete(img);
            cl.remove("selected");
        } else {
            set.add(img);
            cl.add("selected");
        }
        
        let header = Selections.getHeaderToggle(handle);
        this.commitSelectFor(section, header);
    }

    static getHeaderToggle(handle) {
        let image = handle.parentNode;
        let row = image.parentNode;
        let container = row.parentNode;
        let header = container.firstChild;
        return header.firstChild;
    }

    openSelectFor(s, data=null) {
        // create the set if it doesn't exist yet
        if (!this.selected.has(s)) {
            this.selected.set(s, new Set());

        // break up the set and deselect the header if it is full
        } else if (this.selected.get(s) === true) {
            let x = new Set();
            for (const [image,,] of data ?? this.core.bind[s].data)
                x.add(image);
            this.selected.set(s, x);
        }
    }

    commitSelectFor(s, header, data=null) {
        if (this.selected.get(s).size == (data ?? this.core.bind[s].data).length) {
            this.selected.set(s, true);
            if (header)
                header.classList.add("selected");
        } else if (header)
            header.classList.remove("selected");
    }

    subRangeSet(s, start, stop, data, value) {
        this.openSelectFor(s, data);

        const handle = this.core.bind[s]?.handle;
       
        let set = this.selected.get(s);
        for (let i = start; i <= stop; i++) {
            let [img,,] = data[i];
            if (value) set.add(img);
            else       set.delete(img);
        }
        
        if (handle) {
            for (let i = 1; i < handle.children.length; i++) {
                for (let elem of handle.children[i].children) {
                    let img = elem._src;
                    if (set.has(img)) elem.firstChild.classList.add("selected");
                    else elem.firstChild.classList.remove("selected");
                }
            }
        }

        this.commitSelectFor(s, handle?.firstChild?.firstChild, data);
    }

    rangeSet(prev, now, value=true) {
        const [s, l] = Selections.sortLocations(prev, now);

        // expand the range if we selected the header on any of them
        if (s[1] == -1) --s[0];
        if (l[1] == -1) ++l[0];

        if (s[0] == l[0]) {
            this.subRangeSet(s[0], s[1], l[1], s[2], value);
        } else {
            if (s[1] != -1)
                this.subRangeSet(s[0], s[1], s[2].length-1, s[2], value);

            for (let i = s[0] + 1; i < l[0]; i++) {
                this._setHeader(this.core.bind[i]?.handle, i, value);
            }

            if (l[1] != -1)
                this.subRangeSet(l[0], 0, l[1], l[2], value);
        }
    }

    static sortLocations(a, b) {
        if (a[0] < b[0]) {
            return [a, b];     
        } else if (b[0] > a[0]) {
            return [b, a];
        } else if (a[1] < b[1]) {
            return [a, b];
        } else {
            return [b, a];
        }
    }
}

const HeaderHeight = 50;

function verticalViewOf(element) {
    const windowHeight = $(window).height();
    const fromTop = $(document).scrollTop() - $(element).offset().top;

    return [windowHeight, fromTop];
}

function windowOf(view, range) {
    const [windowHeight, fromTop] = view;

    const small = fromTop -      range  * windowHeight;
    const large = fromTop + (1 + range) * windowHeight;

    return [small, large];
}

function partition(section, viewport, idealHeight, safe = false) {
    const idealAspect = viewport / idealHeight;
    
    const shrink = 0.8 * idealAspect;
    const stretch = (safe ? 2 : 1.2) * idealAspect;

    let path = Array(section.length + 1).fill(false);
    path[0] = [0, 0.0, 0.0];
    
    /*
     * i is the last element in the path array, while k is the index of the
     * element in the section itself.
     */
    function update(start, end, cost, height) {
        if (path[end] === false || cost < path[end][1]) {
            path[end] = [start, cost, height];
        }
    }

    for (let start = 0; start < section.length; start++) {
        // If there is no break here, then there is no need to investigate
        if (path[start] === false)
            continue;

        const cost = path[start][1];

        let aspect = 0.0;
        for (let end = start; end < section.length; ) {
            let [name, width, height] = section[end++];
            aspect += width / height;

            // if we reach the end of the section and the row still isn't
            // filled up, we can take a zero penalty row that is exacly the
            // right height
            if (end == section.length && aspect < idealAspect) {
                update(start, end, cost, idealHeight);

            // do the regular bounds checks for strech and shrink
            } else if (aspect >= shrink || safe) {
                if (aspect <= stretch || end == start + 1) {   
                    const rowCost = (aspect - idealAspect) ** 2;
                    update(start, end, cost + rowCost, viewport / aspect);
                } else
                    break;
            }
        }
    }

    let rows = [];
    let end = section.length;
    while (end > 0) {
        if (path[end] === false)
            return partition(section, viewport, idealHeight, true);

        const [start, _, height] = path[end];
        rows.push([start, end, height]);
        end = start;
    }
    
    return rows.reverse();
}

class FullscreenViewer {
    constructor(core) {
        this.core = core;
        this.showHandle = e => this.show(e);

        this.visible = false;

        let page = document.createElement('div');
        page.style.display = "none";
        page.className = "fullscreen";
        page.addEventListener("touchstart", e => this.onClickStart(e), {passive: false});
        page.addEventListener("mousedown", e => this.onClickStart(e), {passive: false});
        page.addEventListener("touchend", e=> this.onClick(e));
        page.addEventListener("mouseup", e => this.onClick(e));
        window.addEventListener("keydown", e => this.onKey(e), {passive: false});

        this.image = document.createElement('img');
        this.image.className = "fullimg";
        this.image.addEventListener("load", () => this.image.style.opacity = "1.0");
        page.append(this.image); 

        this.core.page.append(page);
        this.page = page;
    }

    show(event) {
        if (event.target !== event.currentTarget)
            return;

        const img = event.currentTarget;
        this.section = img._section;
        this.index = img._index;

        this.updateImage();

        this.page.style.display = "block";
        this.visible = true;
        scrollOff();
    }

    hide() {
        this.image.src = "";
        this.page.style.display = "none";
        this.visible = false;
        scrollOn();
    }

    onKey(event) {
        if (this.visible) {
            // left
            if (event.keyCode === 37) {
                this.previous();
            // right
            } else if (event.keyCode === 39) {
                this.next();
            // space
            } else if (event.keyCode === 32) {
                event.preventDefault();
                this.hide();
            }
        }
    }

    onClick(event) {
        const width = this.page.offsetWidth;
        const margin = width * 0.25;
        const pageX = event.pageX ?? event.changedTouches[0].pageX;
        const drag = pageX - this.startX;
        const dragWidth = width * 0.1;

        if (drag > dragWidth) this.previous();
        else if (drag < -dragWidth) this.next();
        else if (width - pageX < margin) this.next();
        else if (pageX < margin) this.previous();
        else this.hide();
    }

    onClickStart(event) {
        this.startX = event.pageX ?? event.changedTouches[0].pageX;
        event.preventDefault();
    }

    _next() {
        const data = this.core.bind[this.section]?.data;
        if (data !== null) {
            if (this.index + 1 < data.length) {
                ++this.index;
                return true;
            } else if (this.section+1 < this.core.bind.length &&
                this.core.bind[this.section+1]?.data !== null) {
                ++this.section;
                this.index = 0;
                return true;
            }
        }
        return false;
    }

    _prev() {
        if (this.core.bind[this.section]?.data !== null) {
            const ndata = this.core.bind[this.section-1]?.data;
            if (this.index > 0) {
                --this.index;
                return true;
            } else if (this.section > 0 && ndata !== null) {
                --this.section;
                this.index = ndata.length-1;
                return true;
            }
        }
        return false;
    }

    next() {
        if (this._next()) {
            this.updateImage();
            this.core.scrollTo(this.section, this.index);
        }
    }

    previous() {
        if (this._prev()) {
            this.updateImage();
            this.core.scrollTo(this.section, this.index);
        }
    }

    updateImage() {
        const data = this.core.bind[this.section].data;
        const image = data[this.index][0];
        this.image.src = `${this.core.resolveUrl(image, "large")}`;
        this.image.style.opacity = "0.8";
    }
}

class DebounceFrame {
    constructor(callback) {
        this.frame = null;
        this.callback = callback;
    }

    service() {
        if (this.frame == null) {
            this.frame = requestAnimationFrame(() => {
                this.frame = null;
                this.callback();
            });
        }
    }

    get on() {
        return this.frame != null;
    }
}

class DebounceTime {
    constructor(timeout, callback) {
        this.timeout = timeout;
        this.timer = null;
        this.callback = callback;
    }

    service() {
        if (this.timer != null) {
            clearTimeout(this.timer);
        }

        this.timer = setTimeout(() => {
            this.timer = null;
            this.callback();
        }, this.timeout);
    }

    get on() {
        return this.timer != null;
    }
}

export class ViewerCore {
    constructor(page, resolveUrl, fetchFragment, head) {
        this.page = page;
        this.resolveUrl = resolveUrl;
        this.fetchFragment = fetchFragment;

        this.select = new Selections(this);
        this.fullscreen = new FullscreenViewer(this);

        this.cancelInstall = false;

        fetchFragment(head)
            .then(data => this.install(data))
            .catch(console.log);
    }

    install(data) {
        if (this.cancelInstall) {
            return true;
        }

        console.log(data);

        this.data = data;

        this.totalHeight = 0.;
        this.heights = Array(data.length).fill(0.);
        this.offsets = Array(data.length + 1).fill(0.);
        this.offsetHead = 0;
        this.offsetHeadLowWaterMark = 0;
        this.oldWidth = this.page.offsetWidth;

        // Incoming requests generate (index, data, row) tuples that are put
        // into the mailbox. These are installed and used when the animation
        // frame is called.
        this.mailbox = [];
        this.bind = Array(data.length).fill(null);
        this.range = [0, 0];

        this.update = new DebounceFrame(() => this.moveWindow());
        this.resize = new DebounceTime(500, () => this._resize());
        this.quality = new DebounceTime(200, () => this.updateQuality());
        
        this.lastScroll = [0, new Date().getTime()];

        this.setIdealHeight();
        this.guessHeights();
        this.moveWindow();

        this.updateService = () => this.update.service();
        this.resizeService = () => this.resize.service();

        window.addEventListener("scroll", this.updateService);
        window.addEventListener("resize", this.resizeService);
    }

    uninstall() {
        this.cancelInstall = true;
        this.page.innerHTML = "";
        window.removeEventListener("scroll", this.updateService);
        window.removeEventListener("resize", this.resizeService);
    }

    removeSection(i) {
        if (this.bind[i])
            this.page.removeChild(this.bind[i].handle);
        this.bind[i] = null;
    }

    createSection(i) {
        let container = document.createElement('div');
        container.className = "section";
        container.style.top = `${this.getOffset(i)}px`;

        let date = new Date(this.data[i][0] * 1000)
            .toLocaleDateString();

        let header = document.createElement('div');
        header.className = "header";
        header.append(this.select.createHeaderToggle(i));
        header.append(date);
        
        container.append(header);
        container.append(this.createPlaceholder(i));

        this.page.append(container);
        this.bind[i] = {
            index: i,
            handle: container,
            data: null
        };

        this.requestSection(i);
    }

    createPlaceholder(i, error=null) {
        let body = document.createElement('div');
        body.className = "placeholder";
        body.style.height = `${this.heights[i] - HeaderHeight}px`;
        body.style.width = "100%";

        if (error) {
            body.innerHTML = error.message;
            body.addEventListener("click", () => this.requestSection(i));
        }

        return body;
    }

    updateSection(msg) {
        if (this.bind[msg.index] === null)
            return;

        // first empty the container
        let container = this.bind[msg.index].handle;
        while (container.children.length > 1)
            container.removeChild(container.lastChild);

        let totalHeight = HeaderHeight;
        for (const [start, end, height] of msg.rows) {
            totalHeight += height;
            let row = document.createElement('div');

            row.className = "row";
            row._highQuality = false;

            let accum = 0.;
            for (let i = start; i < end; i++) {
                let [image, width, oheight] = msg.data[i];
                const scaledWidth = width * height / oheight;

                let img = document.createElement('div');
                img.className = "img";
                img._src = image;
                img._index = i;
                img._section = msg.index;
                img.style.backgroundImage = `url(${this.resolveUrl(image, "small")})`;
                img.style.height = `${height}px`;
                img.style.width = `${scaledWidth}px`;
                img.style.left = accum;

                img.addEventListener("click", this.fullscreen.showHandle);
                img.append(this.select.createToggle(msg.index, image));
                
                row.appendChild(img);
                
                accum += width * height / oheight;
            }

            this.bind[msg.index].handle.append(row);
        }

        this.bind[msg.index].data = msg.data;
        this.setHeight(msg.index, totalHeight);
    }

    failSection(error, i) {
        if (this.bind[i] === null)
            return;

        let container = this.bind[i].handle;
        while (container.children.length > 1)
            container.removeChild(container.lastChild);

        container.append(this.createPlaceholder(i, error));
    }

    requestSection(i) {
        this.fetchFragment(this.data[i][1])
            .then(data => this.receiveSection(data, i))
            .catch(error => this.failSection(error, i));
    }

    receiveSection(data, i) {
        data = data.map(x => {
            x.shift();
            return x;
        });
        console.log(data);

        if (this.bind[i] !== null) {
            let rows = partition(data, this.page.offsetWidth, this.idealHeight);

            this.mailbox.push({
                index: i,
                data: data,
                rows: rows
            });

            this.update.service();
        }
    }

    moveWindow() {
        // get viewable range for window multiples calculations
        const view = verticalViewOf(this.page);

        let [i, j] = this.range;

        const [ut, ub] = windowOf(view, 6);
        while (j > 0                && this.getOffset(j-1) > ub)
            this.removeSection(--j);
        while (i < this.bind.length && this.getOffset(i+1) < ut)
            this.removeSection(i++);

        const [lt, lb] = windowOf(view, 5);
        while (this.getOffset(i) > lt && i > 0)                --i;
        while (this.getOffset(j) < lb && j < this.bind.length) ++j;

        // Commit the new range
        this.range = [i, j];


        let sentinel = i;
        while (this.getOffset(sentinel + 1) < view[1] && sentinel < j)
            sentinel++;
        const fraction = (view[1] - this.getOffset(sentinel)) / this.heights[sentinel];
        this.offsetHeadLowWaterMark = j;


        for (const msg of this.mailbox) {
            this.updateSection(msg);
        }
        this.mailbox = [];

        for (; i < j; i++) {
            if (this.bind[i] === null) {
                this.createSection(i);
            } else {
                this.bind[i].handle.style.top = `${this.getOffset(i)}px`;
            }
        }


        // TODO: this seems to cause scrolling problems on the iPad, I have
        // taken steps to mitigate it, but a better solution would be
        // preferable.
        if (this.offsetHeadLowWaterMark <= sentinel && fraction > 0) {
            const offset = fraction * this.heights[sentinel];
            const delta = view[1] - this.getOffset(sentinel) - offset;
            $(document).scrollTop($(document).scrollTop() - delta);
        }


        let scroll = [view[1], new Date().getTime()];
        const dy_dt = (scroll[0] - this.lastScroll[0]) / (scroll[1] - this.lastScroll[1]);
        this.lastScroll = scroll;

        if (Math.abs(dy_dt) > this.idealHeight * 0.015)
            this.quality.service();

        if (!this.quality.on) {
            this.updateQuality();
        }
        
        this.page.style.height = `${this.totalHeight}px`;
    }

    _resize() {
        const newWidth = this.page.offsetWidth;
        if (Math.abs(this.oldWidth - newWidth) > 1) {
            this.oldWidth = newWidth;

            this.setIdealHeight();

            let [i, j] = this.range;
            for (; i < j; i++) {
                let data = this.bind[i].data;
                if (data !== null) {
                    this.receiveSection(data, i);
                }
            }
        }
    }

    setIdealHeight() {
        const width = this.page.offsetWidth;
        this.idealHeight = Math.min(350, width / 3);
    }

    updateQuality() {
        let [i, j] = this.range;
        for (; i < j; i++) {
            this._updateQuality(i);
        }
    }

    _updateQuality(i) {
        if (this.bind[i] === null)
            return;

        let handle = this.bind[i].handle;
        const view = verticalViewOf(handle);
        const [small, large] = windowOf(view, 1);

        let totalHeight = HeaderHeight;
        for (let i = 1; i < handle.children.length; i++) {
            let row = handle.children[i];
            const height = row.offsetHeight;

            const show = totalHeight < large && totalHeight + height > small;
            if (show != row._highQuality) {
                row._highQuality = show;

                for (let image of row.children) {
                    if (show) {
                        image.style.backgroundImage
                            = `url('${this.resolveUrl(image._src, "medium")}'), url('${this.resolveUrl(image._src, "small")}')`;
                    } else {
                        image.style.backgroundImage
                            = `url('${this.resolveUrl(image._src, "small")}')`;
                    }
                }
            }

            totalHeight += height;
        }
    }

    guessHeights() {
        const heightPerImage = this.idealHeight **2 / this.page.offsetWidth;

        this.totalHeight = 0.;
        for (let i = 0; i < this.data.length; i++) {
            const [,length] = this.data[i];
            const guess = HeaderHeight + Math.max(this.idealHeight, heightPerImage * length);

            this.heights[i] = guess;
            this.totalHeight += guess;
        }
    }
    
    getOffset(i) {
        // calculate the appropriate offset if it hasn't been done already
        for (; this.offsetHead < i; this.offsetHead++) {
            const x = this.offsetHead;
            this.offsets[x+1] =
                this.offsets[x] + this.heights[x];
        }

        return this.offsets[i];
    }

    setHeight(i, value) {
        const diff = value - this.heights[i];
        this.totalHeight += diff;

        this.heights[i] = value;

        // invalidate everything above this
        if (this.offsetHead > i) {
            this.offsetHead = i;
        }
        if (this.offsetHeadLowWaterMark > i) {
            this.offsetHeadLowWaterMark = i;
        }
    }

    scrollTo(section, index) {
        if (this.bind[section]?.data === null) {
            const loc = $(this.page).offset().top + this.getOffset(section)
                + this.heights[section]*index/this.data[section][1];
            $(document).scrollTop(loc);
        } else {
            const handle = this.bind[section].handle;

            let i = 1;
            while (index >= handle.children[i].children.length) {
                index -= handle.children[i++].children.length;
            }

            const row = handle.children[i];
            const loc = $(row).offset().top - ($(window).height() - $(row).height())/2;
            $(document).scrollTop(loc);
        }

        this.update.service();
    }
}

var mobile = (
    // Detect if it is an iPad. If so, then don't disable
    navigator.userAgent.match(/Mac/)
    && navigator.maxTouchPoints
    && navigator.maxTouchPoints > 2
)
    || navigator.userAgent.match(/Android/);

function preventDefault(e) {
    e.preventDefault();
}

function preventDefaultKey(e) {
    if (e.keyCode >= 32 && e.keyCode < 41) {
        e.preventDefault();
        return false;
    }
}

function scrollOff() {
    window.addEventListener("touchmove", preventDefault, {passive: false});
    window.addEventListener("wheel", preventDefault, {passive: false});
    window.addEventListener("keydown", preventDefaultKey, {passive: false});
    if (!mobile) $('body').addClass("noscroll");
}

function scrollOn() {
    window.removeEventListener("touchmove", preventDefault);
    window.removeEventListener("wheel", preventDefault);
    window.removeEventListener("keydown", preventDefaultKey);
    if (!mobile) $('body').removeClass("noscroll");
}
