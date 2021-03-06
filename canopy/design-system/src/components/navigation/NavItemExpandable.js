/**
 * Copyright 2018-2020 Cargill Incorporated
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
import React, { useState } from 'react';
import PropTypes from 'prop-types';
import { NavLink } from 'react-router-dom';

export function NavItemExpandable({ nested, children }) {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <li
      className={`nav-item
                  nav-item-expandable
                  ${isOpen && 'open'}
                  display-flex
                  flexDirection-column
                  justifyContent-center
                  alignItems-flexStart`}
    >
      <div
        className=" label
                    padding-m
                    display-flex
                    flexDirection-row
                    justifyContent-spaceBetween
                    alignItems-center
                    borderWidth-0
                    borderBottom-1
                    borderStyle-solid
                    borderColor-smoke"
        role="button"
        tabIndex="0"
        onKeyPress={e => {
          if (e.key === 'Enter') {
            setIsOpen(!isOpen);
          }
        }}
        onClick={() => setIsOpen(!isOpen)}
      >
        {children}
        <div
          className={`arrow
                      ${isOpen ? 'arrow-up' : 'arrow-down'}`}
        />
      </div>
      <ul
        className={`nested
                      ${isOpen ? 'display-flex' : 'display-none'}
                      flexDirection-column\
                      borderWidth-0
                      borderBottom-1
                      borderStyle-solid
                      borderColor-smoke`}
      >
        {nested.map(child => (
          <NavLink
            to={child.route}
            className="padding-s paddingLeft-l"
            aria-label={child.name}
            key={child.name}
          >
            {child.name}
          </NavLink>
        ))}
      </ul>
    </li>
  );
}

NavItemExpandable.propTypes = {
  children: PropTypes.oneOfType([
    PropTypes.arrayOf(PropTypes.element),
    PropTypes.object
  ]),
  nested: PropTypes.arrayOf(PropTypes.object)
};

NavItemExpandable.defaultProps = {
  children: [],
  nested: []
};
