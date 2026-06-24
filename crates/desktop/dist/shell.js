(()=>{var{defineProperty:h,getOwnPropertyNames:qT,getOwnPropertyDescriptor:GT}=Object,PT=Object.prototype.hasOwnProperty;function ET(T){return this[T]}var bT=(T)=>{var w=(i??=new WeakMap).get(T),F;if(w)return w;if(w=h({},"__esModule",{value:!0}),T&&typeof T==="object"||typeof T==="function"){for(var J of qT(T))if(!PT.call(w,J))h(w,J,{get:ET.bind(T,J),enumerable:!(F=GT(T,J))||F.enumerable})}return i.set(T,w),w},i;var ST=(T)=>T;function kT(T,w){this[T]=ST.bind(null,w)}var xT=(T,w)=>{for(var F in w)h(T,F,{get:w[F],enumerable:!0,configurable:!0,set:kT.bind(w,F)})};var q=function(T,w,F,J){var K=arguments.length,O=K<3?w:J===null?J=Object.getOwnPropertyDescriptor(w,F):J,Q;if(typeof Reflect==="object"&&typeof Reflect.decorate==="function")O=Reflect.decorate(T,w,F,J);else for(var W=T.length-1;W>=0;W--)if(Q=T[W])O=(K<3?Q(O):K>3?Q(w,F,O):Q(w,F))||O;return K>3&&O&&Object.defineProperty(w,F,O),O};var oT={};xT(oT,{GtlShell:()=>u});var L=globalThis,M=L.ShadowRoot&&(L.ShadyCSS===void 0||L.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,p=Symbol(),e=new WeakMap;class R{constructor(T,w,F){if(this._$cssResult$=!0,F!==p)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=T,this.t=w}get styleSheet(){let T=this.o,w=this.t;if(M&&T===void 0){let F=w!==void 0&&w.length===1;F&&(T=e.get(w)),T===void 0&&((this.o=T=new CSSStyleSheet).replaceSync(this.cssText),F&&e.set(w,T))}return T}toString(){return this.cssText}}var s=(T)=>new R(typeof T=="string"?T:T+"",void 0,p),c=(T,...w)=>{let F=T.length===1?T[0]:w.reduce((J,K,O)=>J+((Q)=>{if(Q._$cssResult$===!0)return Q.cssText;if(typeof Q=="number")return Q;throw Error("Value passed to 'css' function must be a 'css' function result: "+Q+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(K)+T[O+1],T[0]);return new R(F,T,p)},t=(T,w)=>{if(M)T.adoptedStyleSheets=w.map((F)=>F instanceof CSSStyleSheet?F:F.styleSheet);else for(let F of w){let J=document.createElement("style"),K=L.litNonce;K!==void 0&&J.setAttribute("nonce",K),J.textContent=F.cssText,T.appendChild(J)}},a=M?(T)=>T:(T)=>T instanceof CSSStyleSheet?((w)=>{let F="";for(let J of w.cssRules)F+=J.cssText;return s(F)})(T):T;var{is:UT,defineProperty:vT,getOwnPropertyDescriptor:yT,getOwnPropertyNames:AT,getOwnPropertySymbols:LT,getPrototypeOf:MT}=Object,g=globalThis,TT=g.trustedTypes,RT=TT?TT.emptyScript:"",gT=g.reactiveElementPolyfillSupport,P=(T,w)=>T,E={toAttribute(T,w){switch(w){case Boolean:T=T?RT:null;break;case Object:case Array:T=T==null?T:JSON.stringify(T)}return T},fromAttribute(T,w){let F=T;switch(w){case Boolean:F=T!==null;break;case Number:F=T===null?null:Number(T);break;case Object:case Array:try{F=JSON.parse(T)}catch(J){F=null}}return F}},j=(T,w)=>!UT(T,w),wT={attribute:!0,type:String,converter:E,reflect:!1,useDefault:!1,hasChanged:j};Symbol.metadata??=Symbol("metadata"),g.litPropertyMetadata??=new WeakMap;class D extends HTMLElement{static addInitializer(T){this._$Ei(),(this.l??=[]).push(T)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(T,w=wT){if(w.state&&(w.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(T)&&((w=Object.create(w)).wrapped=!0),this.elementProperties.set(T,w),!w.noAccessor){let F=Symbol(),J=this.getPropertyDescriptor(T,F,w);J!==void 0&&vT(this.prototype,T,J)}}static getPropertyDescriptor(T,w,F){let{get:J,set:K}=yT(this.prototype,T)??{get(){return this[w]},set(O){this[w]=O}};return{get:J,set(O){let Q=J?.call(this);K?.call(this,O),this.requestUpdate(T,Q,F)},configurable:!0,enumerable:!0}}static getPropertyOptions(T){return this.elementProperties.get(T)??wT}static _$Ei(){if(this.hasOwnProperty(P("elementProperties")))return;let T=MT(this);T.finalize(),T.l!==void 0&&(this.l=[...T.l]),this.elementProperties=new Map(T.elementProperties)}static finalize(){if(this.hasOwnProperty(P("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(P("properties"))){let w=this.properties,F=[...AT(w),...LT(w)];for(let J of F)this.createProperty(J,w[J])}let T=this[Symbol.metadata];if(T!==null){let w=litPropertyMetadata.get(T);if(w!==void 0)for(let[F,J]of w)this.elementProperties.set(F,J)}this._$Eh=new Map;for(let[w,F]of this.elementProperties){let J=this._$Eu(w,F);J!==void 0&&this._$Eh.set(J,w)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(T){let w=[];if(Array.isArray(T)){let F=new Set(T.flat(1/0).reverse());for(let J of F)w.unshift(a(J))}else T!==void 0&&w.push(a(T));return w}static _$Eu(T,w){let F=w.attribute;return F===!1?void 0:typeof F=="string"?F:typeof T=="string"?T.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise((T)=>this.enableUpdating=T),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach((T)=>T(this))}addController(T){(this._$EO??=new Set).add(T),this.renderRoot!==void 0&&this.isConnected&&T.hostConnected?.()}removeController(T){this._$EO?.delete(T)}_$E_(){let T=new Map,w=this.constructor.elementProperties;for(let F of w.keys())this.hasOwnProperty(F)&&(T.set(F,this[F]),delete this[F]);T.size>0&&(this._$Ep=T)}createRenderRoot(){let T=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return t(T,this.constructor.elementStyles),T}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach((T)=>T.hostConnected?.())}enableUpdating(T){}disconnectedCallback(){this._$EO?.forEach((T)=>T.hostDisconnected?.())}attributeChangedCallback(T,w,F){this._$AK(T,F)}_$ET(T,w){let F=this.constructor.elementProperties.get(T),J=this.constructor._$Eu(T,F);if(J!==void 0&&F.reflect===!0){let K=(F.converter?.toAttribute!==void 0?F.converter:E).toAttribute(w,F.type);this._$Em=T,K==null?this.removeAttribute(J):this.setAttribute(J,K),this._$Em=null}}_$AK(T,w){let F=this.constructor,J=F._$Eh.get(T);if(J!==void 0&&this._$Em!==J){let K=F.getPropertyOptions(J),O=typeof K.converter=="function"?{fromAttribute:K.converter}:K.converter?.fromAttribute!==void 0?K.converter:E;this._$Em=J;let Q=O.fromAttribute(w,K.type);this[J]=Q??this._$Ej?.get(J)??Q,this._$Em=null}}requestUpdate(T,w,F,J=!1,K){if(T!==void 0){let O=this.constructor;if(J===!1&&(K=this[T]),F??=O.getPropertyOptions(T),!((F.hasChanged??j)(K,w)||F.useDefault&&F.reflect&&K===this._$Ej?.get(T)&&!this.hasAttribute(O._$Eu(T,F))))return;this.C(T,w,F)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(T,w,{useDefault:F,reflect:J,wrapped:K},O){F&&!(this._$Ej??=new Map).has(T)&&(this._$Ej.set(T,O??w??this[T]),K!==!0||O!==void 0)||(this._$AL.has(T)||(this.hasUpdated||F||(w=void 0),this._$AL.set(T,w)),J===!0&&this._$Em!==T&&(this._$Eq??=new Set).add(T))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(w){Promise.reject(w)}let T=this.scheduleUpdate();return T!=null&&await T,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[J,K]of this._$Ep)this[J]=K;this._$Ep=void 0}let F=this.constructor.elementProperties;if(F.size>0)for(let[J,K]of F){let{wrapped:O}=K,Q=this[J];O!==!0||this._$AL.has(J)||Q===void 0||this.C(J,void 0,K,Q)}}let T=!1,w=this._$AL;try{T=this.shouldUpdate(w),T?(this.willUpdate(w),this._$EO?.forEach((F)=>F.hostUpdate?.()),this.update(w)):this._$EM()}catch(F){throw T=!1,this._$EM(),F}T&&this._$AE(w)}willUpdate(T){}_$AE(T){this._$EO?.forEach((w)=>w.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(T)),this.updated(T)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(T){return!0}update(T){this._$Eq&&=this._$Eq.forEach((w)=>this._$ET(w,this[w])),this._$EM()}updated(T){}firstUpdated(T){}}D.elementStyles=[],D.shadowRootOptions={mode:"open"},D[P("elementProperties")]=new Map,D[P("finalized")]=new Map,gT?.({ReactiveElement:D}),(g.reactiveElementVersions??=[]).push("2.1.2");var o=globalThis,FT=(T)=>T,d=o.trustedTypes,JT=d?d.createPolicy("lit-html",{createHTML:(T)=>T}):void 0;var N=`lit$${Math.random().toFixed(9).slice(2)}$`,YT="?"+N,jT=`<${YT}>`,z=document,S=()=>z.createComment(""),k=(T)=>T===null||typeof T!="object"&&typeof T!="function",n=Array.isArray,dT=(T)=>n(T)||typeof T?.[Symbol.iterator]=="function";var b=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,KT=/-->/g,OT=/>/g,I=RegExp(`>|[ 	
\f\r](?:([^\\s"'>=/]+)([ 	
\f\r]*=[ 	
\f\r]*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),QT=/'/g,WT=/"/g,BT=/^(?:script|style|textarea|title)$/i,l=(T)=>(w,...F)=>({_$litType$:T,strings:w,values:F}),V=l(1),sT=l(2),tT=l(3),Z=Symbol.for("lit-noChange"),B=Symbol.for("lit-nothing"),XT=new WeakMap,f=z.createTreeWalker(z,129);function _T(T,w){if(!n(T)||!T.hasOwnProperty("raw"))throw Error("invalid template strings array");return JT!==void 0?JT.createHTML(w):w}var mT=(T,w)=>{let F=T.length-1,J=[],K,O=w===2?"<svg>":w===3?"<math>":"",Q=b;for(let W=0;W<F;W++){let Y=T[W],A,X,_=-1,C=0;for(;C<Y.length&&(Q.lastIndex=C,X=Q.exec(Y),X!==null);)C=Q.lastIndex,Q===b?X[1]==="!--"?Q=KT:X[1]!==void 0?Q=OT:X[2]!==void 0?(BT.test(X[2])&&(K=RegExp("</"+X[2],"g")),Q=I):X[3]!==void 0&&(Q=I):Q===I?X[0]===">"?(Q=K??b,_=-1):X[1]===void 0?_=-2:(_=Q.lastIndex-X[2].length,A=X[1],Q=X[3]===void 0?I:X[3]==='"'?WT:QT):Q===WT||Q===QT?Q=I:Q===KT||Q===OT?Q=b:(Q=I,K=void 0);let $=Q===I&&T[W+1].startsWith("/>")?" ":"";O+=Q===b?Y+jT:_>=0?(J.push(A),Y.slice(0,_)+"$lit$"+Y.slice(_)+N+$):Y+N+(_===-2?W:$)}return[_T(T,O+(T[F]||"<?>")+(w===2?"</svg>":w===3?"</math>":"")),J]};class x{constructor({strings:T,_$litType$:w},F){let J;this.parts=[];let K=0,O=0,Q=T.length-1,W=this.parts,[Y,A]=mT(T,w);if(this.el=x.createElement(Y,F),f.currentNode=this.el.content,w===2||w===3){let X=this.el.content.firstChild;X.replaceWith(...X.childNodes)}for(;(J=f.nextNode())!==null&&W.length<Q;){if(J.nodeType===1){if(J.hasAttributes())for(let X of J.getAttributeNames())if(X.endsWith("$lit$")){let _=A[O++],C=J.getAttribute(X).split(N),$=/([.?@])?(.*)/.exec(_);W.push({type:1,index:K,name:$[2],strings:C,ctor:$[1]==="."?DT:$[1]==="?"?NT:$[1]==="@"?VT:v}),J.removeAttribute(X)}else X.startsWith(N)&&(W.push({type:6,index:K}),J.removeAttribute(X));if(BT.test(J.tagName)){let X=J.textContent.split(N),_=X.length-1;if(_>0){J.textContent=d?d.emptyScript:"";for(let C=0;C<_;C++)J.append(X[C],S()),f.nextNode(),W.push({type:2,index:++K});J.append(X[_],S())}}}else if(J.nodeType===8)if(J.data===YT)W.push({type:2,index:K});else{let X=-1;for(;(X=J.data.indexOf(N,X+1))!==-1;)W.push({type:7,index:K}),X+=N.length-1}K++}}static createElement(T,w){let F=z.createElement("template");return F.innerHTML=T,F}}function G(T,w,F=T,J){if(w===Z)return w;let K=J!==void 0?F._$Co?.[J]:F._$Cl,O=k(w)?void 0:w._$litDirective$;return K?.constructor!==O&&(K?._$AO?.(!1),O===void 0?K=void 0:(K=new O(T),K._$AT(T,F,J)),J!==void 0?(F._$Co??=[])[J]=K:F._$Cl=K),K!==void 0&&(w=G(T,K._$AS(T,w.values),K,J)),w}class CT{constructor(T,w){this._$AV=[],this._$AN=void 0,this._$AD=T,this._$AM=w}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(T){let{el:{content:w},parts:F}=this._$AD,J=(T?.creationScope??z).importNode(w,!0);f.currentNode=J;let K=f.nextNode(),O=0,Q=0,W=F[0];for(;W!==void 0;){if(O===W.index){let Y;W.type===2?Y=new U(K,K.nextSibling,this,T):W.type===1?Y=new W.ctor(K,W.name,W.strings,this,T):W.type===6&&(Y=new $T(K,this,T)),this._$AV.push(Y),W=F[++Q]}O!==W?.index&&(K=f.nextNode(),O++)}return f.currentNode=z,J}p(T){let w=0;for(let F of this._$AV)F!==void 0&&(F.strings!==void 0?(F._$AI(T,F,w),w+=F.strings.length-2):F._$AI(T[w])),w++}}class U{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(T,w,F,J){this.type=2,this._$AH=B,this._$AN=void 0,this._$AA=T,this._$AB=w,this._$AM=F,this.options=J,this._$Cv=J?.isConnected??!0}get parentNode(){let T=this._$AA.parentNode,w=this._$AM;return w!==void 0&&T?.nodeType===11&&(T=w.parentNode),T}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(T,w=this){T=G(this,T,w),k(T)?T===B||T==null||T===""?(this._$AH!==B&&this._$AR(),this._$AH=B):T!==this._$AH&&T!==Z&&this._(T):T._$litType$!==void 0?this.$(T):T.nodeType!==void 0?this.T(T):dT(T)?this.k(T):this._(T)}O(T){return this._$AA.parentNode.insertBefore(T,this._$AB)}T(T){this._$AH!==T&&(this._$AR(),this._$AH=this.O(T))}_(T){this._$AH!==B&&k(this._$AH)?this._$AA.nextSibling.data=T:this.T(z.createTextNode(T)),this._$AH=T}$(T){let{values:w,_$litType$:F}=T,J=typeof F=="number"?this._$AC(T):(F.el===void 0&&(F.el=x.createElement(_T(F.h,F.h[0]),this.options)),F);if(this._$AH?._$AD===J)this._$AH.p(w);else{let K=new CT(J,this),O=K.u(this.options);K.p(w),this.T(O),this._$AH=K}}_$AC(T){let w=XT.get(T.strings);return w===void 0&&XT.set(T.strings,w=new x(T)),w}k(T){n(this._$AH)||(this._$AH=[],this._$AR());let w=this._$AH,F,J=0;for(let K of T)J===w.length?w.push(F=new U(this.O(S()),this.O(S()),this,this.options)):F=w[J],F._$AI(K),J++;J<w.length&&(this._$AR(F&&F._$AB.nextSibling,J),w.length=J)}_$AR(T=this._$AA.nextSibling,w){for(this._$AP?.(!1,!0,w);T!==this._$AB;){let F=FT(T).nextSibling;FT(T).remove(),T=F}}setConnected(T){this._$AM===void 0&&(this._$Cv=T,this._$AP?.(T))}}class v{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(T,w,F,J,K){this.type=1,this._$AH=B,this._$AN=void 0,this.element=T,this.name=w,this._$AM=J,this.options=K,F.length>2||F[0]!==""||F[1]!==""?(this._$AH=Array(F.length-1).fill(new String),this.strings=F):this._$AH=B}_$AI(T,w=this,F,J){let K=this.strings,O=!1;if(K===void 0)T=G(this,T,w,0),O=!k(T)||T!==this._$AH&&T!==Z,O&&(this._$AH=T);else{let Q=T,W,Y;for(T=K[0],W=0;W<K.length-1;W++)Y=G(this,Q[F+W],w,W),Y===Z&&(Y=this._$AH[W]),O||=!k(Y)||Y!==this._$AH[W],Y===B?T=B:T!==B&&(T+=(Y??"")+K[W+1]),this._$AH[W]=Y}O&&!J&&this.j(T)}j(T){T===B?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,T??"")}}class DT extends v{constructor(){super(...arguments),this.type=3}j(T){this.element[this.name]=T===B?void 0:T}}class NT extends v{constructor(){super(...arguments),this.type=4}j(T){this.element.toggleAttribute(this.name,!!T&&T!==B)}}class VT extends v{constructor(T,w,F,J,K){super(T,w,F,J,K),this.type=5}_$AI(T,w=this){if((T=G(this,T,w,0)??B)===Z)return;let F=this._$AH,J=T===B&&F!==B||T.capture!==F.capture||T.once!==F.once||T.passive!==F.passive,K=T!==B&&(F===B||J);J&&this.element.removeEventListener(this.name,this,F),K&&this.element.addEventListener(this.name,this,T),this._$AH=T}handleEvent(T){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,T):this._$AH.handleEvent(T)}}class $T{constructor(T,w,F){this.element=T,this.type=6,this._$AN=void 0,this._$AM=w,this.options=F}get _$AU(){return this._$AM._$AU}_$AI(T){G(this,T)}}var uT=o.litHtmlPolyfillSupport;uT?.(x,U),(o.litHtmlVersions??=[]).push("3.3.3");var IT=(T,w,F)=>{let J=F?.renderBefore??w,K=J._$litPart$;if(K===void 0){let O=F?.renderBefore??null;J._$litPart$=K=new U(w.insertBefore(S(),O),O,void 0,F??{})}return K._$AI(T),K};var r=globalThis;class H extends D{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let T=super.createRenderRoot();return this.renderOptions.renderBefore??=T.firstChild,T}update(T){let w=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(T),this._$Do=IT(w,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return Z}}H._$litElement$=!0,H.finalized=!0,r.litElementHydrateSupport?.({LitElement:H});var hT=r.litElementPolyfillSupport;hT?.({LitElement:H});(r.litElementVersions??=[]).push("4.2.2");var fT=(T)=>(w,F)=>{F!==void 0?F.addInitializer(()=>{customElements.define(T,w)}):customElements.define(T,w)};var pT={attribute:!0,type:String,converter:E,reflect:!1,hasChanged:j},cT=(T=pT,w,F)=>{let{kind:J,metadata:K}=F,O=globalThis.litPropertyMetadata.get(K);if(O===void 0&&globalThis.litPropertyMetadata.set(K,O=new Map),J==="setter"&&((T=Object.create(T)).wrapped=!0),O.set(F.name,T),J==="accessor"){let{name:Q}=F;return{set(W){let Y=w.get.call(this);w.set.call(this,W),this.requestUpdate(Q,Y,T,!0,W)},init(W){return W!==void 0&&this.C(Q,void 0,T,W),W}}}if(J==="setter"){let{name:Q}=F;return function(W){let Y=this[Q];w.call(this,W),this.requestUpdate(Q,Y,T,!0,W)}}throw Error("Unsupported decorator location: "+J)};function zT(T){return(w,F)=>typeof F=="object"?cT(T,w,F):((J,K,O)=>{let Q=K.hasOwnProperty(O);return K.constructor.createProperty(O,J),Q?Object.getOwnPropertyDescriptor(K,O):void 0})(T,w,F)}function y(T){return zT({...T,state:!0,attribute:!1})}function ZT(T,w){if(w<0||w>=T.tabs.length)return T;let F=T.tabs.filter((K,O)=>O!==w);if(F.length===0)return{tabs:F,active:0,showHistory:T.showHistory};let J=aT(T.active,w,F.length);return{tabs:F,active:J,showHistory:T.showHistory}}function aT(T,w,F){if(w<T)return T-1;if(w===T)return Math.min(w,F-1);return Math.min(T,F-1)}function HT(){let w=window.__TAURI__;if(typeof w!=="object"||w===null||typeof w.core!=="object"||typeof w.event!=="object")throw Error("__TAURI__ is not available — must run inside a Tauri webview");return w}class u extends H{constructor(){super(...arguments);this.tabs=[];this.active=0;this.showHistory=!1;this.history=[]}static styles=c`
    :host {
      display: grid;
      grid-template-rows: auto 1fr;
      height: 100%;
      background: #1e1e1e;
      color: #d4d4d4;
      font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto,
        "Helvetica Neue", Arial, sans-serif;
      font-size: 14px;
      -webkit-font-smoothing: antialiased;
    }
    .mono {
      font-family: ui-monospace, "SF Mono", "JetBrains Mono", "Fira Code", Menlo,
        Consolas, monospace;
    }
    .tabs {
      display: flex;
      gap: 2px;
      background: #181818;
      padding: 6px 8px 0;
      overflow-x: auto;
      border-bottom: 1px solid #2d2d2d;
    }
    .tab {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      min-width: 0;
      padding: 7px 8px 7px 14px;
      color: #9d9d9d;
      background: #232323;
      border-radius: 7px 7px 0 0;
      cursor: pointer;
      white-space: nowrap;
      font-family: ui-monospace, "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace;
      font-size: 12.5px;
      transition: background 0.12s ease, color 0.12s ease;
    }
    .tab:hover { color: #d4d4d4; }
    .tab[active] { background: #1e1e1e; color: #fff; box-shadow: inset 0 2px 0 #569cd6; }
    .tab-label {
      max-width: 22ch;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .tab-close {
      display: grid;
      place-items: center;
      width: 18px;
      height: 18px;
      padding: 0;
      border: 0;
      border-radius: 4px;
      color: #858585;
      background: transparent;
      cursor: pointer;
      font: inherit;
      line-height: 1;
    }
    .tab-close:hover {
      color: #fff;
      background: #3a3d3f;
    }
    .tab.history { margin-left: auto; font-family: inherit; font-weight: 600; }
    iframe { border: 0; width: 100%; height: 100%; background: #1e1e1e; }
    .panel { padding: 18px 22px; overflow: auto; }
    .panel code {
      font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
      background: #2a2a2a;
      padding: 1px 6px;
      border-radius: 4px;
    }
    .repo {
      margin: 20px 0 8px;
      font-size: 12px;
      font-weight: 700;
      letter-spacing: 0.06em;
      text-transform: uppercase;
      color: #569cd6;
    }
    .repo:first-child { margin-top: 0; }
    .row {
      padding: 9px 12px;
      cursor: pointer;
      border-radius: 6px;
      border: 1px solid transparent;
      line-height: 1.5;
    }
    .row:hover { background: #2a2d2e; border-color: #333; }
    .row small {
      color: #858585;
      font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
      font-size: 11.5px;
    }
  `;_unlisten;connectedCallback(){super.connectedCallback();let{core:T,event:w}=HT();T.invoke("drain_pending_diffs").then((F)=>F.forEach((J)=>this.openTab(J))).catch(console.error),w.listen("open-diff",(F)=>this.openTab(F.payload)).then((F)=>{this._unlisten=F}).catch(console.error)}disconnectedCallback(){super.disconnectedCallback(),this._unlisten?.(),this._unlisten=void 0}labelFor(T){return T.replace("diff://","").slice(0,12)}openTab(T,w=this.labelFor(T)){let F=this.tabs.findIndex((J)=>J.url===T);if(F>=0){let J=this.tabs[F];if(J!==void 0&&J.label!==w)this.tabs=this.tabs.map((K,O)=>O===F?{...K,label:w}:K);this.active=F,this.showHistory=!1;return}this.tabs=[...this.tabs,{url:T,label:w}],this.active=this.tabs.length-1,this.showHistory=!1}closeTab(T){let w=ZT({tabs:this.tabs,active:this.active,showHistory:this.showHistory},T);this.tabs=[...w.tabs],this.active=w.active,this.showHistory=w.showHistory}historyTabLabel(T){let w=T.title.trim();return w!==""&&w!=="diff"&&w!=="merge-diff"?w:this.labelFor(T.url)}async openHistory(){let{core:T}=HT();this.history=await T.invoke("list_history"),this.showHistory=!0}render(){return V`
      <div class="tabs">
        ${this.tabs.map((T,w)=>V`
            <div
              class="tab"
              ?active=${!this.showHistory&&w===this.active}
              @click=${()=>{this.active=w,this.showHistory=!1}}
              title=${T.label}
            >
              <span class="tab-label">${T.label}</span>
              <button
                class="tab-close"
                type="button"
                aria-label=${`Close ${T.label}`}
                title="Close tab"
                @click=${(F)=>{F.stopPropagation(),this.closeTab(w)}}
              >
                ×
              </button>
            </div>
          `)}
        <div
          class="tab history"
          ?active=${this.showHistory}
          @click=${()=>this.openHistory()}
        >
          History
        </div>
      </div>
      ${this.showHistory?this.renderHistory():this.renderContent()}
    `}renderContent(){let T=this.tabs[this.active];return T?V`<iframe src=${T.url}></iframe>`:V`<div class="panel">
          No diff open. Run <code>gtl diff</code> or pick from History.
        </div>`}renderHistory(){let T=new Map;for(let w of this.history){let F=w.repo_name||w.repo_id,J=T.get(F);if(J!==void 0)J.push(w);else T.set(F,[w])}return V`<div class="panel">
      ${[...T.entries()].map(([w,F])=>V`
          <div class="repo">${w}</div>
          ${F.map((J)=>V`
              <div class="row" @click=${()=>this.openTab(J.url,this.historyTabLabel(J))}>
                ${J.title} · <small>${J.range_label} · ${J.head_committed_at}</small>
              </div>
            `)}
        `)}
    </div>`}}q([y()],u.prototype,"tabs",void 0),q([y()],u.prototype,"active",void 0),q([y()],u.prototype,"showHistory",void 0),q([y()],u.prototype,"history",void 0),u=q([fT("gtl-shell")],u);})();
